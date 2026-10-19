// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Derived Render Graph: scene→render bindings and partial rebuild.
//!
//! [`RenderGraph`] owns a [`graph::Graph`] (dirty propagation,
//! revisions, topo order) and adds:
//!
//! - DERIVATION: each scene node produces a chain
//!   `Source → Transform → [Effect…] → [Mask] → Blend`, sibling groups fold
//!   into background/foreground chains, the single `Output` consumes the
//!   last top;
//! - BINDINGS (`source_map`, `tops`, `bindings`, `synced`) telling,
//!   for each scene node, which render nodes depend on it and at
//!   which scene revision they are synchronized;
//! - incremental SYNC: modified attribute → dirty without rebuild;
//!   add/remove/reorder within a scope → local rewiring; any other
//!   structural change → rebuild of THE scope (sibling group), never
//!   of the whole graph. The scope consumer node (group-`Blend` or
//!   `Output`) always survives (stable identity for backends).
//!
//! Rebuilding the graph recomputes no pixels: only dirty flags
//! (read via [`RenderGraph::plan_dirty`]) drive the backend's work.

use std::collections::{HashMap, HashSet};

use graph::Graph as DepGraph;
use ids::Revision;
use scene::{Mask as SceneMask, NodeKind, Scene};

use crate::{
    Backend, EffectKey, EffectOverlay, RenderEdge, RenderNode, RenderNodeId, RenderOp, RenderPort,
    SceneNodeId,
};

/// Derivation, synchronization or execution failure.
///
/// Internal derivation is acyclic by construction: [`RenderError::Cycle`]
/// only occurs on inconsistent manual use (defensive).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// Scene node unknown at derivation time.
    #[error("unknown scene node: {0}")]
    UnknownScene(SceneNodeId),
    /// Cyclic dependency (defensive — derivation produces none).
    #[error("cyclic render dependency: {from} -> {to}")]
    Cycle {
        /// Requested upstream.
        from: RenderNodeId,
        /// Requested downstream.
        to: RenderNodeId,
    },
    /// Unknown render node (stale plan between two `sync`s).
    #[error("unknown render node: {0}")]
    UnknownRender(RenderNodeId),
    /// The backend failed during execution.
    #[error("backend failed: {0}")]
    Backend(String),
}

/// What `sync` did — observability with no effect on rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncReport {
    /// Added scene nodes (derived chains, without touching the existing).
    pub appended: usize,
    /// Removed scene nodes (derivatives purged, neighbors rewired).
    pub removed: usize,
    /// Fully rebuilt scopes (sibling groups).
    pub rebuilt_scopes: usize,
    /// Scopes reordered by rewiring alone (identities preserved).
    pub reordered: usize,
    /// Attribute-modified scene nodes (dirty, zero rebuild).
    pub dirtied: usize,
}

impl SyncReport {
    /// True if nothing changed (graph already synchronized).
    #[must_use]
    pub fn is_clean(self) -> bool {
        self == Self::default()
    }

    fn absorb(&mut self, other: Self) {
        self.appended += other.appended;
        self.removed += other.removed;
        self.rebuilt_scopes += other.rebuilt_scopes;
        self.reordered += other.reordered;
        self.dirtied += other.dirtied;
    }
}

/// Execution plan: nodes in topological order (upstream first).
/// Snapshot — recompute after each `sync` touching the structure.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    /// Nodes to execute, in order.
    pub order: Vec<RenderNodeId>,
}

impl ExecutionPlan {
    /// Number of scheduled nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// True if the plan is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// Execution report (see [`RenderGraph::execute`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecuteReport {
    /// Nodes submitted to the backend.
    pub executed: usize,
}

/// Derivation signature of a scene node: everything which, when changed,
/// REQUIRES a rebuild (the rest = attribute → dirty only).
///
/// Fine nature (`TextData`, effect parameters…) is excluded: rewritten
/// text dirties its chain without rebuilding it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Binding {
    tag: u8,
    parent: Option<SceneNodeId>,
    children: Vec<SceneNodeId>,
    masks: usize,
    effects: Vec<EffectKey>,
}

/// Render Graph: derived operations + bindings + invalidation engine.
///
/// The [`RenderOp::Output`] sink is created by [`RenderGraph::new`] and
/// survives all `sync`s (stable identity for backends).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderGraph {
    inner: DepGraph,
    nodes: HashMap<RenderNodeId, RenderNode>,
    incoming: HashMap<RenderNodeId, Vec<RenderEdge>>,
    source_map: HashMap<SceneNodeId, Vec<RenderNodeId>>,
    tops: HashMap<SceneNodeId, RenderNodeId>,
    bindings: HashMap<SceneNodeId, Binding>,
    synced: HashMap<SceneNodeId, Revision>,
    root_order: Vec<SceneNodeId>,
    output: RenderNodeId,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderGraph {
    /// Empty graph with its `Output` sink (no scene derived).
    #[must_use]
    pub fn new() -> Self {
        let mut inner = DepGraph::new();
        let output = RenderNodeId::new();
        inner.insert(output.as_graph_id());
        let mut nodes = HashMap::new();
        nodes.insert(output, RenderNode::new(output, RenderOp::Output, None));
        let mut incoming = HashMap::new();
        incoming.insert(output, Vec::new());
        Self {
            inner,
            nodes,
            incoming,
            source_map: HashMap::new(),
            tops: HashMap::new(),
            bindings: HashMap::new(),
            synced: HashMap::new(),
            root_order: Vec::new(),
            output,
        }
    }

    /// Derives the full graph from a scene (+ effect overlay).
    #[must_use]
    pub fn build(scene: &Scene, overlay: &EffectOverlay) -> Self {
        let mut graph = Self::new();
        let empty_exit = HashMap::new();
        let empty_removed = HashSet::new();
        let empty_bg: HashMap<RenderNodeId, Option<RenderNodeId>> = HashMap::new();
        let _ = graph.reconcile_scope(scene, overlay, None, &empty_exit, &empty_removed, &empty_bg);
        for group in scene_groups_dfs(scene) {
            let _ = graph.reconcile_scope(
                scene,
                overlay,
                Some(group),
                &empty_exit,
                &empty_removed,
                &empty_bg,
            );
        }
        graph
    }

    /// Single final sink (stable identity across `sync`s).
    #[must_use]
    pub fn output(&self) -> RenderNodeId {
        self.output
    }

    /// Number of render nodes (sink included).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of typed edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Read-only node (`None` if unknown).
    #[must_use]
    pub fn find(&self, id: RenderNodeId) -> Option<&RenderNode> {
        self.nodes.get(&id)
    }

    /// True if the node is tracked.
    #[must_use]
    pub fn contains(&self, id: RenderNodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Incoming edges (zero allocation — slice over the stored).
    #[must_use]
    pub fn inputs_of(&self, id: RenderNodeId) -> Option<&[RenderEdge]> {
        self.incoming.get(&id).map(Vec::as_slice)
    }

    /// Render nodes derived from a scene node, in derivation order
    /// (upstream chain first — `mapped[0]` = `Source`).
    #[must_use]
    pub fn derived_of(&self, scene: SceneNodeId) -> Option<&[RenderNodeId]> {
        self.source_map.get(&scene).map(Vec::as_slice)
    }

    /// Top (`Blend`) of a scene node (`None` if not derived).
    #[must_use]
    pub fn top_of(&self, scene: SceneNodeId) -> Option<RenderNodeId> {
        self.tops.get(&scene).copied()
    }

    /// Dirtiness of a node (`None` if unknown).
    #[must_use]
    pub fn is_dirty(&self, id: RenderNodeId) -> Option<bool> {
        self.inner.is_dirty(id.as_graph_id())
    }

    /// Revision of a node (`None` if unknown — single source: the graph).
    #[must_use]
    pub fn node_revision(&self, id: RenderNodeId) -> Option<Revision> {
        self.inner.node_revision(id.as_graph_id())
    }

    /// Last emitted revision.
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        self.inner.tree_revision()
    }

    /// Dirties a node and its downstream (manual invalidation — `sync` is the
    /// normal path from the scene).
    pub fn mark_dirty(&mut self, id: RenderNodeId) -> Option<Revision> {
        self.inner.mark_dirty(id.as_graph_id())
    }

    /// Marks a node recomputed (see `graph::Graph::clear_dirty`: no bump).
    pub fn clear_dirty(&mut self, id: RenderNodeId) -> Option<bool> {
        self.inner.clear_dirty(id.as_graph_id())
    }

    // -- Synchronization --------------------------------------------------

    /// Synchronizes from the scene (+ overlay): purges the vanished,
    /// rebuilds structurally changed scopes, dirties the
    /// attribute-modified ones. Idempotent: called again with no change, the report
    /// is clean ([`SyncReport::is_clean`]).
    #[must_use = "le rapport dit ce qui a changé"]
    pub fn sync(&mut self, scene: &Scene, overlay: &EffectOverlay) -> SyncReport {
        let mut report = SyncReport::default();
        // Phase A: purge the vanished (exit backgrounds captured BEFORE).
        let gone: Vec<SceneNodeId> = self
            .bindings
            .keys()
            .copied()
            .filter(|id| !scene.contains(*id))
            .collect();
        let mut removed = HashSet::new();
        let mut exit = HashMap::new();
        // Pre-purge backgrounds of ALL nodes: the removal scrub
        // erases incoming edges; this table allows rewiring.
        let prev_bg: HashMap<RenderNodeId, Option<RenderNodeId>> = self
            .nodes
            .keys()
            .copied()
            .map(|rid| (rid, self.background_of(rid)))
            .collect();
        for sid in &gone {
            if let Some(list) = self.source_map.get(sid) {
                for rid in list {
                    exit.insert(*rid, self.background_of(*rid));
                    removed.insert(*rid);
                }
            }
        }
        for sid in gone {
            self.remove_scene_derived(sid);
            report.removed += 1;
        }
        // Phase B: scopes top-down (roots then groups in DFS).
        report.absorb(self.reconcile_scope(scene, overlay, None, &exit, &removed, &prev_bg));
        for group in scene_groups_dfs(scene) {
            report.absorb(self.reconcile_scope(
                scene,
                overlay,
                Some(group),
                &exit,
                &removed,
                &prev_bg,
            ));
        }
        report
    }

    // -- Plans and execution -------------------------------------------------

    /// Full plan: all nodes in topological order.
    #[must_use]
    pub fn plan(&self) -> ExecutionPlan {
        ExecutionPlan {
            order: self
                .inner
                .topo_order()
                .into_iter()
                .map(RenderNodeId::from_graph_id)
                .filter(|id| self.nodes.contains_key(id))
                .collect(),
        }
    }

    /// Incremental plan: only dirty nodes, in topological order.
    /// Prepares a future scheduler (today: clean nodes skipped).
    #[must_use]
    pub fn plan_dirty(&self) -> ExecutionPlan {
        ExecutionPlan {
            order: self
                .inner
                .topo_order()
                .into_iter()
                .map(RenderNodeId::from_graph_id)
                .filter(|id| {
                    self.nodes.contains_key(id)
                        && self.inner.is_dirty(id.as_graph_id()).unwrap_or(false)
                })
                .collect(),
        }
    }

    /// Executes a plan via the backend (frame opened/closed by the backend;
    /// error propagated with no observable partial state graph-side).
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownRender`] if the plan references a vanished node
    /// (stale plan between two `sync`s), [`RenderError::Backend`] if the
    /// backend fails.
    pub fn execute<B: Backend>(
        &self,
        backend: &mut B,
        plan: &ExecutionPlan,
    ) -> Result<ExecuteReport, RenderError> {
        backend.begin_frame();
        let mut executed = 0;
        for id in &plan.order {
            let node = self.nodes.get(id).ok_or(RenderError::UnknownRender(*id))?;
            let inputs = self.inputs_of(*id).unwrap_or(&[]);
            backend
                .execute_node(node, inputs)
                .map_err(RenderError::Backend)?;
            executed += 1;
        }
        backend.end_frame();
        Ok(ExecuteReport { executed })
    }

    // -- Derivation ----------------------------------------------------------

    /// Derives the subtree of `sid`, chained onto `bg` (background), and returns
    /// its top (`Blend`). Bindings are recorded along the way.
    fn derive_subtree(
        &mut self,
        scene: &Scene,
        overlay: &EffectOverlay,
        sid: SceneNodeId,
        bg: Option<RenderNodeId>,
    ) -> Option<RenderNodeId> {
        let node = scene.find(sid)?;
        if node.is_group() {
            let mut child_bg = None;
            let mut last = None;
            for child in node.children.clone() {
                if let Some(top) = self.derive_subtree(scene, overlay, child, child_bg) {
                    child_bg = Some(top);
                    last = Some(top);
                }
            }
            let blend = self.add_render_node(RenderOp::Blend, Some(sid));
            if let Some(fg) = last {
                self.link(fg, blend, RenderPort::Foreground);
            }
            if let Some(b) = bg {
                self.link(b, blend, RenderPort::Background);
            }
            self.tops.insert(sid, blend);
            self.store_binding(scene, overlay, sid);
            Some(blend)
        } else {
            let src = self.add_render_node(RenderOp::Source, Some(sid));
            let xfm = self.add_render_node(RenderOp::Transform, Some(sid));
            self.link(src, xfm, RenderPort::Foreground);
            let mut cur = xfm;
            if let Some(effects) = overlay.get(&sid) {
                for key in effects.clone() {
                    let effect = self.add_render_node(RenderOp::Effect(key), Some(sid));
                    self.link(cur, effect, RenderPort::Foreground);
                    cur = effect;
                }
            }
            let has_masks = scene.masks_of(sid).is_some_and(|m| !m.is_empty());
            if has_masks {
                let mask = self.add_render_node(RenderOp::Mask, Some(sid));
                self.link(cur, mask, RenderPort::Foreground);
                cur = mask;
            }
            let blend = self.add_render_node(RenderOp::Blend, Some(sid));
            self.link(cur, blend, RenderPort::Foreground);
            if let Some(b) = bg {
                self.link(b, blend, RenderPort::Background);
            }
            self.tops.insert(sid, blend);
            self.store_binding(scene, overlay, sid);
            Some(blend)
        }
    }

    /// Reconciles ONE scope (sibling group): `None` = roots, `Some(g)` =
    /// children of group `g`. The consumer (sink or group `Blend`)
    /// always survives — only its wiring changes.
    fn reconcile_scope(
        &mut self,
        scene: &Scene,
        overlay: &EffectOverlay,
        parent: Option<SceneNodeId>,
        exit: &HashMap<RenderNodeId, Option<RenderNodeId>>,
        removed: &HashSet<RenderNodeId>,
        prev_bg: &HashMap<RenderNodeId, Option<RenderNodeId>>,
    ) -> SyncReport {
        let report = SyncReport::default();
        if parent.is_some_and(|p| !scene.contains(p)) {
            return report;
        }
        let cur: Vec<SceneNodeId> = match parent {
            None => scene.roots().to_vec(),
            Some(p) => scene
                .children_of(p)
                .map_or_else(Vec::new, <[SceneNodeId]>::to_vec),
        };
        let prev: Vec<SceneNodeId> = match parent {
            None => self.root_order.clone(),
            Some(p) => self
                .bindings
                .get(&p)
                .map_or_else(Vec::new, |b| b.children.clone()),
        };
        if cur == prev {
            if self.scope_fresh(scene, overlay, parent, &cur) {
                return self.attribute_pass(scene, &cur);
            }
            // Same sibling group but stale binding (masks/effects/parent):
            // chain shapes changed → rebuild.
            return self.rebuild_scope(scene, overlay, parent, &cur);
        }
        // Freshness of the kept existing (new nodes have no binding).
        let shared = prev_intersect_cur(&prev, &cur);
        if self.scope_fresh(scene, overlay, parent, &shared) {
            if is_permutation(&prev, &cur) {
                return self.reorder_scope(parent, &cur);
            }
            if cur.starts_with(&prev) {
                return self.append_scope(scene, overlay, parent, &prev, &cur);
            }
            if is_subsequence(&prev, &cur) {
                return self.removal_scope(parent, &prev, &cur, exit, removed, prev_bg);
            }
        }
        self.rebuild_scope(scene, overlay, parent, &cur)
    }

    /// Are the members' bindings intact (besides order)?
    fn scope_fresh(
        &self,
        scene: &Scene,
        overlay: &EffectOverlay,
        parent: Option<SceneNodeId>,
        members: &[SceneNodeId],
    ) -> bool {
        members.iter().all(|sid| {
            self.bindings.get(sid).is_some_and(|b| {
                b.tag == kind_tag_of(scene, *sid)
                    && b.parent == parent
                    && b.masks == scene.masks_of(*sid).map_or(0, <[SceneMask]>::len)
                    && overlay
                        .get(sid)
                        .map_or(b.effects.is_empty(), |v| *v == b.effects)
            })
        })
    }

    /// Attribute pass: dirties the derivatives of nodes whose scene
    /// revision advanced (zero rebuild).
    fn attribute_pass(&mut self, scene: &Scene, scope: &[SceneNodeId]) -> SyncReport {
        let mut report = SyncReport::default();
        for sid in scope_subtree_ids(scene, scope) {
            let Some(rev) = scene.node_revision(sid) else {
                continue;
            };
            let seen = self.synced.get(&sid).copied().unwrap_or(Revision::NONE);
            if !rev.is_newer_than(seen) {
                continue;
            }
            if let Some(first) = self
                .source_map
                .get(&sid)
                .and_then(|mapped| mapped.first().copied())
            {
                let gid = first.as_graph_id();
                if !self.inner.is_dirty(gid).unwrap_or(true) {
                    self.inner.mark_dirty(gid);
                }
            }
            self.synced.insert(sid, rev);
            report.dirtied += 1;
        }
        report
    }

    /// Pure append at scope end: derives the new nodes, rewires the
    /// consumer (the existing is untouched).
    fn append_scope(
        &mut self,
        scene: &Scene,
        overlay: &EffectOverlay,
        parent: Option<SceneNodeId>,
        prev: &[SceneNodeId],
        cur: &[SceneNodeId],
    ) -> SyncReport {
        let mut report = SyncReport::default();
        let mut bg = prev.last().and_then(|sid| self.tops.get(sid).copied());
        let old_last = bg;
        for sid in &cur[prev.len()..] {
            if let Some(top) = self.derive_subtree(scene, overlay, *sid, bg) {
                bg = Some(top);
                report.appended += 1;
            }
        }
        self.rewire_consumer(parent, old_last, bg);
        self.store_scope_order(parent, cur);
        report
    }

    /// Pure removal (survivor order preserved): rewires the orphaned
    /// backgrounds to the captured exit backgrounds, then the consumer.
    /// The purged nodes' incoming edges having been erased, the pre-purge
    /// backgrounds (`prev_bg`) guide the rewiring.
    fn removal_scope(
        &mut self,
        parent: Option<SceneNodeId>,
        prev: &[SceneNodeId],
        cur: &[SceneNodeId],
        exit: &HashMap<RenderNodeId, Option<RenderNodeId>>,
        removed: &HashSet<RenderNodeId>,
        prev_bg: &HashMap<RenderNodeId, Option<RenderNodeId>>,
    ) -> SyncReport {
        let report = SyncReport {
            removed: prev.len() - cur.len(),
            ..SyncReport::default()
        };
        for sid in cur {
            let Some(top) = self.tops.get(sid).copied() else {
                continue;
            };
            let Some(former) = prev_bg.get(&top).copied().flatten() else {
                continue;
            };
            if !removed.contains(&former) {
                continue;
            }
            let target = resolve_exit(former, exit, removed, |id| self.background_of(id));
            let live = self.background_of(top);
            if live != target {
                if let Some(old) = live {
                    self.remove_render_edge(old, top);
                }
                if let Some(bg) = target {
                    self.link(bg, top, RenderPort::Background);
                }
            }
        }
        let old_last = self.consumer_foreground(parent);
        let new_last = cur.last().and_then(|sid| self.tops.get(sid).copied());
        self.rewire_consumer(parent, old_last, new_last);
        self.store_scope_order(parent, cur);
        report
    }

    /// Current Foreground input of the scope consumer (`None` if none).
    fn consumer_foreground(&self, parent: Option<SceneNodeId>) -> Option<RenderNodeId> {
        let consumer = match parent {
            None => self.output,
            Some(p) => self.tops.get(&p).copied()?,
        };
        self.foreground_of(consumer)
    }

    /// Current Foreground input of a node (`None` if none).
    fn foreground_of(&self, id: RenderNodeId) -> Option<RenderNodeId> {
        self.incoming.get(&id).and_then(|edges| {
            edges
                .iter()
                .find(|e| e.port == RenderPort::Foreground)
                .map(|e| e.from)
        })
    }

    /// Pure reorder: rewires backgrounds and consumer, identities preserved.
    fn reorder_scope(&mut self, parent: Option<SceneNodeId>, cur: &[SceneNodeId]) -> SyncReport {
        let mut report = SyncReport::default();
        let mut bg = None;
        for sid in cur {
            if let Some(top) = self.tops.get(sid).copied() {
                let current_bg = self.background_of(top);
                if current_bg != bg {
                    if let Some(old) = current_bg {
                        self.remove_render_edge(old, top);
                    }
                    if let Some(new_bg) = bg {
                        self.link(new_bg, top, RenderPort::Background);
                    }
                }
                bg = Some(top);
            }
        }
        let current_fg = self.consumer_foreground(parent);
        if current_fg != bg {
            self.rewire_consumer(parent, current_fg, bg);
        }
        self.store_scope_order(parent, cur);
        report.reordered = 1;
        report
    }

    /// Scope rebuild: purges the sibling group's derivatives,
    /// re-derives, rewires the consumer (itself preserved).
    fn rebuild_scope(
        &mut self,
        scene: &Scene,
        overlay: &EffectOverlay,
        parent: Option<SceneNodeId>,
        cur: &[SceneNodeId],
    ) -> SyncReport {
        let mut report = SyncReport::default();
        let old_last = self.scope_last_top(parent);
        for sid in scope_subtree_ids(scene, &self.previous_scope_members(parent)) {
            self.remove_scene_derived(sid);
        }
        let mut bg = None;
        for sid in cur {
            if let Some(top) = self.derive_subtree(scene, overlay, *sid, bg) {
                bg = Some(top);
            }
        }
        self.rewire_consumer(parent, old_last, bg);
        self.store_scope_order(parent, cur);
        report.rebuilt_scopes = 1;
        report
    }

    /// Former members of a scope (order stored before purge).
    fn previous_scope_members(&self, parent: Option<SceneNodeId>) -> Vec<SceneNodeId> {
        match parent {
            None => self.root_order.clone(),
            Some(p) => self
                .bindings
                .get(&p)
                .map_or_else(Vec::new, |b| b.children.clone()),
        }
    }

    /// Last top of a scope before rebuild (for the consumer).
    fn scope_last_top(&self, parent: Option<SceneNodeId>) -> Option<RenderNodeId> {
        let prev = self.previous_scope_members(parent);
        prev.last().and_then(|sid| self.tops.get(sid).copied())
    }

    // -- Low-level wiring ------------------------------------------------

    /// Rewires the consumer's Foreground edge (`old → new`, `None`
    /// = absent). The root consumer is the sink; the group
    /// consumer is its `Blend` (preserved across rebuilds).
    fn rewire_consumer(
        &mut self,
        parent: Option<SceneNodeId>,
        old: Option<RenderNodeId>,
        new: Option<RenderNodeId>,
    ) {
        if old == new {
            return;
        }
        let consumer = match parent {
            None => self.output,
            Some(p) => match self.tops.get(&p).copied() {
                Some(blend) => blend,
                None => return,
            },
        };
        if let Some(from) = old {
            self.remove_render_edge(from, consumer);
        }
        if let Some(to) = new {
            // Avoids the duplicate if the edge already exists (partial reorders).
            let exists = self.incoming.get(&consumer).is_some_and(|edges| {
                edges
                    .iter()
                    .any(|e| e.from == to && e.port == RenderPort::Foreground)
            });
            if !exists {
                self.link(to, consumer, RenderPort::Foreground);
            }
        }
    }

    /// Current background of a node (`None` if none).
    fn background_of(&self, id: RenderNodeId) -> Option<RenderNodeId> {
        self.incoming.get(&id).and_then(|edges| {
            edges
                .iter()
                .find(|e| e.port == RenderPort::Background)
                .map(|e| e.from)
        })
    }

    /// Adds a typed edge (graph mirror included). Derivation is
    /// acyclic by construction — failure is defensive (debug_assert).
    fn link(&mut self, from: RenderNodeId, to: RenderNodeId, port: RenderPort) {
        self.incoming
            .entry(to)
            .or_default()
            .push(RenderEdge::new(from, to, port));
        let ok = self
            .inner
            .add_edge(from.as_graph_id(), to.as_graph_id())
            .is_ok();
        debug_assert!(ok, "render derivation is acyclic by construction");
    }

    /// Removes a typed edge (absent = no effect).
    fn remove_render_edge(&mut self, from: RenderNodeId, to: RenderNodeId) {
        if let Some(edges) = self.incoming.get_mut(&to) {
            edges.retain(|e| e.from != from);
        }
        self.inner.remove_edge(from.as_graph_id(), to.as_graph_id());
    }

    /// Creates a render node + clean bindings.
    fn add_render_node(&mut self, op: RenderOp, scene: Option<SceneNodeId>) -> RenderNodeId {
        let id = RenderNodeId::new();
        self.inner.insert(id.as_graph_id());
        self.nodes.insert(id, RenderNode::new(id, op, scene));
        self.incoming.insert(id, Vec::new());
        if let Some(sid) = scene {
            self.source_map.entry(sid).or_default().push(id);
        }
        id
    }

    /// Removes a render node and its edges (graph mirror + slices).
    fn remove_render_node(&mut self, id: RenderNodeId) {
        self.inner.remove_node(id.as_graph_id());
        self.nodes.remove(&id);
        self.incoming.remove(&id);
        for edges in self.incoming.values_mut() {
            edges.retain(|e| e.from != id);
        }
        self.source_map.retain(|_, list| {
            list.retain(|&rid| rid != id);
            !list.is_empty()
        });
        self.tops.retain(|_, top| *top != id);
    }

    /// Purges a scene node's derivatives + its bindings.
    fn remove_scene_derived(&mut self, sid: SceneNodeId) {
        if let Some(list) = self.source_map.remove(&sid) {
            for rid in list {
                self.remove_render_node(rid);
            }
        }
        self.tops.remove(&sid);
        self.bindings.remove(&sid);
        self.synced.remove(&sid);
    }

    /// Records the binding + synchronized revision of a derived node.
    fn store_binding(&mut self, scene: &Scene, overlay: &EffectOverlay, sid: SceneNodeId) {
        let Some(node) = scene.find(sid) else {
            return;
        };
        self.bindings.insert(
            sid,
            Binding {
                tag: kind_tag(&node.kind),
                parent: node.parent,
                children: node.children.clone(),
                masks: node.masks.len(),
                effects: overlay.get(&sid).cloned().unwrap_or_default(),
            },
        );
        self.synced
            .insert(sid, scene.node_revision(sid).unwrap_or(Revision::NONE));
    }

    /// Memorizes a scope's order after a structural mutation.
    fn store_scope_order(&mut self, parent: Option<SceneNodeId>, cur: &[SceneNodeId]) {
        match parent {
            None => self.root_order = cur.to_vec(),
            Some(p) => {
                if let Some(binding) = self.bindings.get_mut(&p) {
                    binding.children = cur.to_vec();
                }
            }
        }
    }
}

// -- Free helpers ----------------------------------------------------------

/// Nature discriminant (fine content — `TextData`… — is an attribute).
fn kind_tag(kind: &NodeKind) -> u8 {
    match kind {
        NodeKind::Group => 0,
        NodeKind::Image => 1,
        NodeKind::Shape(_) => 2,
        NodeKind::Text(_) => 3,
        NodeKind::Video => 4,
        NodeKind::Layout => 5,
    }
}

/// Current-nature discriminant (`u8::MAX` if node unknown).
fn kind_tag_of(scene: &Scene, sid: SceneNodeId) -> u8 {
    scene.find(sid).map_or(u8::MAX, |n| kind_tag(&n.kind))
}

/// Groups in DFS from the roots (parents before children — deterministic).
fn scene_groups_dfs(scene: &Scene) -> Vec<SceneNodeId> {
    let mut groups = Vec::new();
    let mut stack: Vec<SceneNodeId> = scene.roots().iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        if let Some(node) = scene.find(id) {
            if node.is_group() {
                groups.push(id);
            }
            stack.extend(node.children.iter().rev().copied());
        }
    }
    groups
}

/// All scene nodes of a sibling group's subtrees (included, in order).
fn scope_subtree_ids(scene: &Scene, scope: &[SceneNodeId]) -> Vec<SceneNodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<SceneNodeId> = scope.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        out.push(id);
        if let Some(node) = scene.find(id) {
            stack.extend(node.children.iter().rev().copied());
        }
    }
    out
}

/// Intersection `prev ∩ cur` in `prev`'s order (freshness of the existing).
fn prev_intersect_cur(prev: &[SceneNodeId], cur: &[SceneNodeId]) -> Vec<SceneNodeId> {
    prev.iter().copied().filter(|id| cur.contains(id)).collect()
}

/// Same set, different order (pure reorder).
fn is_permutation(prev: &[SceneNodeId], cur: &[SceneNodeId]) -> bool {
    cur.len() == prev.len() && cur.iter().all(|id| prev.contains(id))
}

/// `cur` = `prev` amputated, order preserved (pure removal).
fn is_subsequence(prev: &[SceneNodeId], cur: &[SceneNodeId]) -> bool {
    if cur.len() >= prev.len() {
        return false;
    }
    let mut rest = cur.iter();
    let mut want = rest.next();
    for id in prev {
        if Some(id) == want {
            want = rest.next();
        }
    }
    want.is_none()
}

/// Follows exit backgrounds through purged nodes (removal rewiring).
fn resolve_exit(
    mut current: RenderNodeId,
    exit: &HashMap<RenderNodeId, Option<RenderNodeId>>,
    removed: &HashSet<RenderNodeId>,
    live_bg: impl Fn(RenderNodeId) -> Option<RenderNodeId>,
) -> Option<RenderNodeId> {
    let mut guard = 0;
    loop {
        match exit.get(&current) {
            Some(Some(bg)) if removed.contains(bg) && guard < 4096 => {
                current = *bg;
                guard += 1;
            }
            Some(bg) => return *bg,
            None => return live_bg(current),
        }
    }
}
