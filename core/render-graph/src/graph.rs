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

//! Render Graph dérivé : liaisons scène→rendu et reconstruction partielle.
//!
//! [`RenderGraph`] possède un [`graph::Graph`] (propagation dirty,
//! révisions, ordre topo) et ajoute :
//!
//! - la DÉRIVATION : chaque nœud de scène produit une chaîne
//!   `Source → Transform → [Effect…] → [Mask] → Blend`, les fratries se
//!   plient en chaîne fond/premier-plan, l'unique `Output` consomme le
//!   dernier top ;
//! - les LIAISONS (`source_map`, `tops`, `bindings`, `synced`) qui disent,
//!   pour chaque nœud de scène, quels nœuds de rendu en dépendent et à
//!   quelle révision de scène ils sont synchronisés ;
//! - le SYNC incrémental : attribut modifié → dirty sans reconstruction ;
//!   ajout/retrait/réordre dans une portée → re-câblage local ; autre
//!   changement structurel → reconstruction de LA portée (fratrie), jamais
//!   du graphe entier. Le nœud consommateur de portée (groupe-`Blend` ou
//!   `Output`) survit toujours (identité stable pour les backends).
//!
//! Reconstruire le graphe ne recalcule aucun pixel : seuls les flags dirty
//! (lus via [`RenderGraph::plan_dirty`]) pilotent le travail du backend.

use std::collections::{HashMap, HashSet};

use graph::Graph as DepGraph;
use ids::Revision;
use scene::{Mask as SceneMask, NodeKind, Scene};

use crate::{
    Backend, EffectKey, EffectOverlay, RenderEdge, RenderNode, RenderNodeId, RenderOp, RenderPort,
    SceneNodeId,
};

/// Échec de dérivation, de synchronisation ou d'exécution.
///
/// La dérivation interne est acyclique par construction : [`RenderError::Cycle`]
/// ne survient que sur usage manuel incohérent (défensif).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// Nœud de scène inconnu au moment de dériver.
    #[error("unknown scene node: {0}")]
    UnknownScene(SceneNodeId),
    /// Dépendance cyclique (défensif — la dérivation n'en produit pas).
    #[error("cyclic render dependency: {from} -> {to}")]
    Cycle {
        /// Amont demandé.
        from: RenderNodeId,
        /// Aval demandé.
        to: RenderNodeId,
    },
    /// Nœud de rendu inconnu (plan périmé entre deux `sync`).
    #[error("unknown render node: {0}")]
    UnknownRender(RenderNodeId),
    /// Le backend a échoué pendant l'exécution.
    #[error("backend failed: {0}")]
    Backend(String),
}

/// Ce que `sync` a fait — observabilité sans effet sur le rendu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncReport {
    /// Nœuds de scène ajoutés (chaînes dérivées, sans toucher l'existant).
    pub appended: usize,
    /// Nœuds de scène supprimés (dérivés purgés, voisins re-câblés).
    pub removed: usize,
    /// Portées entièrement reconstruites (fratries).
    pub rebuilt_scopes: usize,
    /// Portées réordonnées par re-câblage seul (identités préservées).
    pub reordered: usize,
    /// Nœuds de scène attribut-modifiés (dirty, zéro reconstruction).
    pub dirtied: usize,
}

impl SyncReport {
    /// Vrai si rien n'a changé (graphe déjà synchronisé).
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

/// Plan d'exécution : nœuds en ordre topologique (amont d'abord).
/// Instantané — à recalculer après chaque `sync` qui touche la structure.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    /// Nœuds à exécuter, en ordre.
    pub order: Vec<RenderNodeId>,
}

impl ExecutionPlan {
    /// Nombre de nœuds planifiés.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Vrai si le plan est vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// Bilan d'exécution (voir [`RenderGraph::execute`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecuteReport {
    /// Nœuds soumis au backend.
    pub executed: usize,
}

/// Signature de dérivation d'un nœud de scène : tout ce qui, en changeant,
/// EXIGE une reconstruction (le reste = attribut → dirty seul).
///
/// La nature fine (`TextData`, paramètres d'effets…) en est exclue : un
/// texte réécrit salit sa chaîne sans la reconstruire.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Binding {
    tag: u8,
    parent: Option<SceneNodeId>,
    children: Vec<SceneNodeId>,
    masks: usize,
    effects: Vec<EffectKey>,
}

/// Render Graph : opérations dérivées + liaisons + moteur d'invalidation.
///
/// Le puits [`RenderOp::Output`] est créé par [`RenderGraph::new`] et
/// survit à tous les `sync` (identité stable pour les backends).
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
    /// Graphe vide avec son puits `Output` (aucune scène dérivée).
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

    /// Dérive le graphe complet d'une scène (+ overlay d'effets).
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

    /// Puits final unique (identité stable entre les `sync`).
    #[must_use]
    pub fn output(&self) -> RenderNodeId {
        self.output
    }

    /// Nombre de nœuds de rendu (puits inclus).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Nombre d'arêtes typées.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Nœud en lecture seule (`None` si inconnu).
    #[must_use]
    pub fn find(&self, id: RenderNodeId) -> Option<&RenderNode> {
        self.nodes.get(&id)
    }

    /// Vrai si le nœud est suivi.
    #[must_use]
    pub fn contains(&self, id: RenderNodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Arêtes entrantes (zéro allocation — tranche sur le stocké).
    #[must_use]
    pub fn inputs_of(&self, id: RenderNodeId) -> Option<&[RenderEdge]> {
        self.incoming.get(&id).map(Vec::as_slice)
    }

    /// Nœuds de rendu dérivés d'un nœud de scène, dans l'ordre de dérivation
    /// (chaîne amont d'abord — `mapped[0]` = `Source`).
    #[must_use]
    pub fn derived_of(&self, scene: SceneNodeId) -> Option<&[RenderNodeId]> {
        self.source_map.get(&scene).map(Vec::as_slice)
    }

    /// Top (`Blend`) d'un nœud de scène (`None` si non dérivé).
    #[must_use]
    pub fn top_of(&self, scene: SceneNodeId) -> Option<RenderNodeId> {
        self.tops.get(&scene).copied()
    }

    /// Saleté d'un nœud (`None` si inconnu).
    #[must_use]
    pub fn is_dirty(&self, id: RenderNodeId) -> Option<bool> {
        self.inner.is_dirty(id.as_graph_id())
    }

    /// Révision d'un nœud (`None` si inconnu — source unique : le graphe).
    #[must_use]
    pub fn node_revision(&self, id: RenderNodeId) -> Option<Revision> {
        self.inner.node_revision(id.as_graph_id())
    }

    /// Dernière révision émise.
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        self.inner.tree_revision()
    }

    /// Salit un nœud et son aval (invalidation manuelle — `sync` est la
    /// voie normale depuis la scène).
    pub fn mark_dirty(&mut self, id: RenderNodeId) -> Option<Revision> {
        self.inner.mark_dirty(id.as_graph_id())
    }

    /// Marque un nœud recalculé (voir `graph::Graph::clear_dirty` : sans bump).
    pub fn clear_dirty(&mut self, id: RenderNodeId) -> Option<bool> {
        self.inner.clear_dirty(id.as_graph_id())
    }

    // -- Synchronisation ---------------------------------------------------

    /// Synchronise depuis la scène (+ overlay) : purge les disparus,
    /// reconstruit les portées structurellement changées, salit les
    /// attribut-modifiés. Idempotent : rappelé sans changement, le rapport
    /// est vierge ([`SyncReport::is_clean`]).
    #[must_use = "le rapport dit ce qui a changé"]
    pub fn sync(&mut self, scene: &Scene, overlay: &EffectOverlay) -> SyncReport {
        let mut report = SyncReport::default();
        // Phase A : purge des disparus (fonds de sortie capturés AVANT).
        let gone: Vec<SceneNodeId> = self
            .bindings
            .keys()
            .copied()
            .filter(|id| !scene.contains(*id))
            .collect();
        let mut removed = HashSet::new();
        let mut exit = HashMap::new();
        // Fonds pré-purge de TOUS les nœuds : le scrub de suppression
        // efface les arêtes entrantes, cette table permet de re-câbler.
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
        // Phase B : portées de haut en bas (racines puis groupes en DFS).
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

    // -- Plans et exécution --------------------------------------------------

    /// Plan complet : tous les nœuds en ordre topologique.
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

    /// Plan incrémental : seuls les nœuds dirty, en ordre topologique.
    /// Prépare un futur scheduler (aujourd'hui : saut des propres).
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

    /// Exécute un plan via le backend ( Cadre ouvert/fermé par le backend ;
    /// erreur remontée sans état partiel observable côté graphe).
    ///
    /// # Errors
    ///
    /// [`RenderError::UnknownRender`] si le plan référence un nœud disparu
    /// (plan périmé entre deux `sync`), [`RenderError::Backend`] si le
    /// backend échoue.
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

    // -- Dérivation ----------------------------------------------------------

    /// Dérive le sous-arbre de `sid`, chaîné sur `bg` (fond), et retourne
    /// son top (`Blend`). Les liaisons sont enregistrées au fil de l'eau.
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

    /// Réconcilie UNE portée (fratrie) : `None` = racines, `Some(g)` =
    /// enfants du groupe `g`. Le consommateur (puits ou `Blend` du groupe)
    /// survit toujours — seul son câblage change.
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
            // Même fratrie mais liaison périmée (masques/effets/parent) :
            // la forme des chaînes a changé → reconstruction.
            return self.rebuild_scope(scene, overlay, parent, &cur);
        }
        // Fraîcheur de l'existant conservé (les nouveaux n'ont pas de liaison).
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

    /// Les liaisons des membres sont-elles intactes (hors ordre) ?
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

    /// Passe attributs : salit les dérivés des nœuds dont la révision de
    /// scène a avancé (zéro reconstruction).
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

    /// Ajout pur en fin de portée : dérive les nouveaux, re-câble le
    /// consommateur (l'existant est intouché).
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

    /// Retrait pur (ordre des survivants préservé) : re-câble les fonds
    /// orphelins vers les fonds de sortie capturés, puis le consommateur.
    /// Les arêtes entrantes des nœuds purgés ayant été effacées, les fonds
    /// d'avant-purge (`prev_bg`) guident le re-câblage.
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

    /// Entrée Foreground actuelle du consommateur de portée (`None` si aucune).
    fn consumer_foreground(&self, parent: Option<SceneNodeId>) -> Option<RenderNodeId> {
        let consumer = match parent {
            None => self.output,
            Some(p) => self.tops.get(&p).copied()?,
        };
        self.foreground_of(consumer)
    }

    /// Entrée Foreground actuelle d'un nœud (`None` si aucune).
    fn foreground_of(&self, id: RenderNodeId) -> Option<RenderNodeId> {
        self.incoming.get(&id).and_then(|edges| {
            edges
                .iter()
                .find(|e| e.port == RenderPort::Foreground)
                .map(|e| e.from)
        })
    }

    /// Réordre pur : re-câble fonds et consommateur, identités préservées.
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

    /// Reconstruction de portée : purge les dérivés de la fratrie,
    /// re-dérive, re-câble le consommateur (lui-même préservé).
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

    /// Anciens membres d'une portée (ordre stocké avant purge).
    fn previous_scope_members(&self, parent: Option<SceneNodeId>) -> Vec<SceneNodeId> {
        match parent {
            None => self.root_order.clone(),
            Some(p) => self
                .bindings
                .get(&p)
                .map_or_else(Vec::new, |b| b.children.clone()),
        }
    }

    /// Dernier top d'une portée avant reconstruction (pour le consommateur).
    fn scope_last_top(&self, parent: Option<SceneNodeId>) -> Option<RenderNodeId> {
        let prev = self.previous_scope_members(parent);
        prev.last().and_then(|sid| self.tops.get(sid).copied())
    }

    // -- Câblage de bas niveau ------------------------------------------------

    /// Re-câble l'arête Foreground du consommateur (`old → new`, `None`
    /// = absence). Le consommateur racine est le puits ; le consommateur
    /// de groupe est son `Blend` (préservé par les reconstructions).
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
            // Évite le doublon si l'arête existe déjà (réordres partiels).
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

    /// Fond actuel d'un nœud (`None` si aucun).
    fn background_of(&self, id: RenderNodeId) -> Option<RenderNodeId> {
        self.incoming.get(&id).and_then(|edges| {
            edges
                .iter()
                .find(|e| e.port == RenderPort::Background)
                .map(|e| e.from)
        })
    }

    /// Ajoute une arête typée (miroir graphe inclus). La dérivation est
    /// acyclique par construction — l'échec est défensif (debug_assert).
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

    /// Retire une arête typée (absente = sans effet).
    fn remove_render_edge(&mut self, from: RenderNodeId, to: RenderNodeId) {
        if let Some(edges) = self.incoming.get_mut(&to) {
            edges.retain(|e| e.from != from);
        }
        self.inner.remove_edge(from.as_graph_id(), to.as_graph_id());
    }

    /// Crée un nœud de rendu + liaisons vierges.
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

    /// Supprime un nœud de rendu et ses arêtes (miroir graphe + tranches).
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

    /// Purge les dérivés d'un nœud de scène + ses liaisons.
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

    /// Enregistre liaison + révision synchronisée d'un nœud dérivé.
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

    /// Mémorise l'ordre d'une portée après mutation structurelle.
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

// -- Helpers libres ----------------------------------------------------------

/// Discriminant de nature (le contenu fin — `TextData`… — est attribut).
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

/// Discriminant de la nature actuelle (`u8::MAX` si nœud inconnu).
fn kind_tag_of(scene: &Scene, sid: SceneNodeId) -> u8 {
    scene.find(sid).map_or(u8::MAX, |n| kind_tag(&n.kind))
}

/// Groupes en DFS depuis les racines (parents avant enfants — déterministe).
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

/// Tous les nœuds de scène des sous-arbres d'une fratrie (inclus, en ordre).
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

/// Intersection `prev ∩ cur` dans l'ordre de `prev` (fraîcheur de l'existant).
fn prev_intersect_cur(prev: &[SceneNodeId], cur: &[SceneNodeId]) -> Vec<SceneNodeId> {
    prev.iter().copied().filter(|id| cur.contains(id)).collect()
}

/// Même ensemble, ordre différent (réordre pur).
fn is_permutation(prev: &[SceneNodeId], cur: &[SceneNodeId]) -> bool {
    cur.len() == prev.len() && cur.iter().all(|id| prev.contains(id))
}

/// `cur` = `prev` amputé, ordre préservé (retrait pur).
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

/// Suit les fonds de sortie à travers les nœuds purgés (re-câblage retrait).
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
