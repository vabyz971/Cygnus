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

//! Scene graph: semantic hierarchy, sole source of structure.
//!
//! [`Scene`] owns all [`SceneNode`]s (`HashMap` table + list of
//! roots): it maintains the `parent`/`children` consistency and
//! allocates [`Revision`]s (see [`crate::revision`]).
//!
//! Mutation contract (uniform): they return
//! [`Option<Revision>`] — `None` = target not found or rejected operation
//! (cycle, impossible re-attachment); `Some(revision)` = success, the
//! revision being NEW if something changed, UNCHANGED (the node's
//! current value) on a no-effect write. Removals
//! return a `bool` (removed nodes have no revision anymore).
//!
//! No public `find_mut` method: every mutation goes through `Scene`
//! to guarantee revisions. Read-only access via [`Scene::find`].

use std::collections::HashMap;

use math_utils::Transform2D;

use crate::{Affine2, Mask, NodeKind, Revision, SceneNode, SceneNodeId};

/// Semantic hierarchy of a document's visual nodes.
///
/// Not thread-safe by itself (standard `HashMap`): inter-thread sharing
/// stays the caller's job (`Arc`, dedicated worker…), as with the suite's
/// existing documents.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    nodes: HashMap<SceneNodeId, SceneNode>,
    roots: Vec<SceneNodeId>,
    next: u64,
}

impl Scene {
    /// Empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes (roots + descendants).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the scene holds no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// True if `id` designates a scene node.
    #[must_use]
    pub fn contains(&self, id: SceneNodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Roots in document order.
    #[must_use]
    pub fn roots(&self) -> &[SceneNodeId] {
        &self.roots
    }

    /// All identifiers (order not guaranteed — deterministic walk via
    /// [`Scene::roots`] + [`Scene::children_of`]).
    #[must_use]
    pub fn all_ids(&self) -> Vec<SceneNodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Last emitted revision ([`Revision::NONE`] if no mutation).
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        Revision(self.next)
    }

    /// Current revision of a node (`None` if not found).
    #[must_use]
    pub fn node_revision(&self, id: SceneNodeId) -> Option<Revision> {
        self.nodes.get(&id).map(|n| n.revision)
    }

    // -- Creation -----------------------------------------------------------

    /// Creates a root node (defaults: identity, visible,
    /// full opacity) and returns its fresh identifier.
    pub fn create_node(&mut self, kind: NodeKind) -> SceneNodeId {
        let id = SceneNodeId::new();
        let revision = self.alloc();
        self.nodes.insert(id, SceneNode::new(id, kind, revision));
        self.roots.push(id);
        id
    }

    /// Creates a node directly as a child of `parent`
    /// (`None` if the parent is not found).
    pub fn create_child(&mut self, parent: SceneNodeId, kind: NodeKind) -> Option<SceneNodeId> {
        if !self.nodes.contains_key(&parent) {
            return None;
        }
        let id = SceneNodeId::new();
        let revision = self.alloc();
        let mut node = SceneNode::new(id, kind, revision);
        node.parent = Some(parent);
        self.nodes.insert(id, node);
        if let Some(slot) = self.nodes.get_mut(&parent) {
            slot.children.push(id);
        }
        self.touch(parent);
        Some(id)
    }

    // -- Reading ------------------------------------------------------------

    /// Node by identifier (`None` if not found).
    #[must_use]
    pub fn find(&self, id: SceneNodeId) -> Option<&SceneNode> {
        self.nodes.get(&id)
    }

    /// Direct parent (`None` if root or not found — see
    /// [`Scene::contains`] to tell apart).
    #[must_use]
    pub fn parent_of(&self, id: SceneNodeId) -> Option<SceneNodeId> {
        self.nodes.get(&id)?.parent
    }

    /// Direct children in document order (`None` if not found).
    #[must_use]
    pub fn children_of(&self, id: SceneNodeId) -> Option<&[SceneNodeId]> {
        self.nodes.get(&id).map(|n| n.children.as_slice())
    }

    /// Semantic nature (`None` if not found).
    #[must_use]
    pub fn kind_of(&self, id: SceneNodeId) -> Option<&NodeKind> {
        self.nodes.get(&id).map(|n| &n.kind)
    }

    /// Local transform (`None` if not found).
    #[must_use]
    pub fn local_transform(&self, id: SceneNodeId) -> Option<Transform2D> {
        self.nodes.get(&id).map(|n| n.local)
    }

    /// Own visibility (excluding ancestors — see [`Scene::world_visible`).
    #[must_use]
    pub fn is_visible(&self, id: SceneNodeId) -> Option<bool> {
        self.nodes.get(&id).map(|n| n.visible)
    }

    /// Own opacity `0.0..=100.0` (excluding ancestors).
    #[must_use]
    pub fn opacity(&self, id: SceneNodeId) -> Option<f32> {
        self.nodes.get(&id).map(|n| n.opacity)
    }

    /// Attached masks (`None` if node not found).
    #[must_use]
    pub fn masks_of(&self, id: SceneNodeId) -> Option<&[Mask]> {
        self.nodes.get(&id).map(|n| n.masks.as_slice())
    }

    /// Free metadata entry (`None` if node or key not found).
    #[must_use]
    pub fn meta(&self, id: SceneNodeId, key: &str) -> Option<&str> {
        self.nodes.get(&id)?.metadata.get(key).map(String::as_str)
    }

    /// Depth (`0` = root, `None` if not found).
    #[must_use]
    pub fn depth(&self, id: SceneNodeId) -> Option<usize> {
        let mut depth = 0;
        let mut current = self.nodes.get(&id)?;
        while let Some(parent) = current.parent {
            depth += 1;
            current = self.nodes.get(&parent)?;
        }
        Some(depth)
    }

    /// Subtree size, node included (`None` if not found).
    #[must_use]
    pub fn subtree_len(&self, id: SceneNodeId) -> Option<usize> {
        if !self.nodes.contains_key(&id) {
            return None;
        }
        let mut count = 0;
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if let Some(node) = self.nodes.get(&current) {
                count += 1;
                stack.extend(node.children.iter().copied());
            }
        }
        Some(count)
    }

    /// True if `ancestor` is a strict ancestor of `id`.
    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: SceneNodeId, id: SceneNodeId) -> bool {
        let mut current = self.nodes.get(&id).and_then(|n| n.parent);
        while let Some(node_id) = current {
            if node_id == ancestor {
                return true;
            }
            current = self.nodes.get(&node_id).and_then(|n| n.parent);
        }
        false
    }

    // -- Structure ----------------------------------------------------------

    /// Attaches `child` under `parent`, at sibling-group end (moved from
    /// its former parent or roots as applicable).
    ///
    /// Rejected (`None`) if an identifier is not found, if
    /// `parent == child`, or if the attachment would create a cycle. Already
    /// last of the sibling group: no effect (current revision returned).
    /// Same parent but another position: moved to the end (reorder).
    pub fn attach(&mut self, parent: SceneNodeId, child: SceneNodeId) -> Option<Revision> {
        if parent == child || !self.contains(parent) || !self.contains(child) {
            return None;
        }
        let already_last = self.nodes.get(&parent).is_some_and(|p| {
            p.children.last().is_some_and(|&last| last == child)
                && self
                    .nodes
                    .get(&child)
                    .is_some_and(|n| n.parent == Some(parent))
        });
        if already_last {
            return self.node_revision(child);
        }
        if self.is_ancestor_of(child, parent) {
            return None;
        }
        let old_parent = self.nodes.get(&child).and_then(|n| n.parent);
        self.unlink(child);
        if let Some(slot) = self.nodes.get_mut(&parent) {
            slot.children.push(child);
        }
        if let Some(node) = self.nodes.get_mut(&child) {
            node.parent = Some(parent);
        }
        let revision = self.touch(child);
        // The old branch lost a subtree: it changes too.
        if let Some(old) = old_parent {
            self.touch(old);
        }
        Some(revision)
    }

    /// Detaches `child` from its parent (becomes a root again, subtree kept).
    ///
    /// `None` if not found, already a root, or… `None` in both cases —
    /// see [`Scene::contains`] to tell apart.
    pub fn detach(&mut self, child: SceneNodeId) -> Option<Revision> {
        let old_parent = {
            let node = self.nodes.get(&child)?;
            node.parent?
        };
        self.unlink(child);
        if let Some(node) = self.nodes.get_mut(&child) {
            node.parent = None;
        }
        self.roots.push(child);
        self.touch(old_parent);
        Some(self.touch(child))
    }

    /// Removes `id` and all its descendants. False if not found.
    pub fn remove_subtree(&mut self, id: SceneNodeId) -> bool {
        if !self.nodes.contains_key(&id) {
            return false;
        }
        let mut condemned = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if let Some(node) = self.nodes.get(&current) {
                stack.extend(node.children.iter().copied());
                condemned.push(current);
            }
        }
        let parent = self.nodes.get(&id).and_then(|n| n.parent);
        match parent {
            Some(owner) => {
                self.unlink(id);
                for dead in condemned {
                    self.nodes.remove(&dead);
                }
                self.touch(owner);
            }
            None => {
                self.roots.retain(|&r| r != id);
                for dead in condemned {
                    self.nodes.remove(&dead);
                }
                self.advance();
            }
        }
        true
    }

    // -- Attributes ----------------------------------------------------------

    /// Replaces the local transform (revision unchanged if identical value).
    pub fn set_local_transform(&mut self, id: SceneNodeId, local: Transform2D) -> Option<Revision> {
        if !self.nodes.contains_key(&id) {
            return None;
        }
        if self.nodes.get(&id).is_some_and(|n| n.local == local) {
            return self.node_revision(id);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.local = local;
        }
        Some(self.touch(id))
    }

    /// Replaces visibility (revision unchanged if identical value).
    pub fn set_visible(&mut self, id: SceneNodeId, visible: bool) -> Option<Revision> {
        if !self.nodes.contains_key(&id) {
            return None;
        }
        if self.nodes.get(&id).is_some_and(|n| n.visible == visible) {
            return self.node_revision(id);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.visible = visible;
        }
        Some(self.touch(id))
    }

    /// Replaces opacity, clamped to `0.0..=100.0` (revision unchanged if
    /// identical value after clamping).
    pub fn set_opacity(&mut self, id: SceneNodeId, opacity: f32) -> Option<Revision> {
        let clamped = opacity.clamp(0.0, 100.0);
        if !self.nodes.contains_key(&id) {
            return None;
        }
        if self.nodes.get(&id).is_some_and(|n| n.opacity == clamped) {
            return self.node_revision(id);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.opacity = clamped;
        }
        Some(self.touch(id))
    }

    /// Adds or replaces (same `Mask::id`) a mask of the node.
    pub fn add_mask(&mut self, owner: SceneNodeId, mask: Mask) -> Option<Revision> {
        if !self.nodes.contains_key(&owner) {
            return None;
        }
        if let Some(node) = self.nodes.get_mut(&owner) {
            if let Some(slot) = node.masks.iter_mut().find(|m| m.id == mask.id) {
                if *slot == mask {
                    let current = node.revision;
                    return Some(current);
                }
                *slot = mask;
            } else {
                node.masks.push(mask);
            }
        }
        Some(self.touch(owner))
    }

    /// Removes a mask (`Some` without bumping if already absent).
    pub fn remove_mask(&mut self, owner: SceneNodeId, mask_id: SceneNodeId) -> Option<Revision> {
        let position = self
            .nodes
            .get(&owner)?
            .masks
            .iter()
            .position(|m| m.id == mask_id);
        match position {
            None => self.node_revision(owner),
            Some(index) => {
                if let Some(node) = self.nodes.get_mut(&owner) {
                    node.masks.remove(index);
                }
                Some(self.touch(owner))
            }
        }
    }

    /// Sets a metadata entry (revision unchanged if identical pair).
    pub fn set_meta(
        &mut self,
        id: SceneNodeId,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Option<Revision> {
        let (key, value) = (key.into(), value.into());
        if !self.nodes.contains_key(&id) {
            return None;
        }
        if self
            .nodes
            .get(&id)
            .and_then(|n| n.metadata.get(&key))
            .is_some_and(|v| *v == value)
        {
            return self.node_revision(id);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.metadata.insert(key, value);
        }
        Some(self.touch(id))
    }

    /// Removes a metadata entry (`Some` without bumping if key absent).
    pub fn remove_meta(&mut self, id: SceneNodeId, key: &str) -> Option<Revision> {
        if !self.nodes.contains_key(&id) {
            return None;
        }
        if self
            .nodes
            .get(&id)
            .is_some_and(|n| !n.metadata.contains_key(key))
        {
            return self.node_revision(id);
        }
        if let Some(node) = self.nodes.get_mut(&id) {
            node.metadata.remove(key);
        }
        Some(self.touch(id))
    }

    // -- World derivatives (never stored) ---------------------------------

    /// World matrix: composition of locals from the root down to `id`.
    #[must_use]
    pub fn world_affine(&self, id: SceneNodeId) -> Option<Affine2> {
        let mut chain = Vec::new();
        let mut current = self.nodes.get(&id)?;
        loop {
            chain.push(Affine2::from_transform(&current.local));
            match current.parent {
                Some(parent) => current = self.nodes.get(&parent)?,
                None => break,
            }
        }
        let mut world = Affine2::identity();
        for local in chain.iter().rev() {
            world = world.concat(*local);
        }
        Some(world)
    }

    /// Local point of `id` expressed in world coordinates (`None` if unknown).
    #[must_use]
    pub fn world_point(&self, id: SceneNodeId, x: f32, y: f32) -> Option<(f32, f32)> {
        self.world_affine(id).map(|w| w.apply(x, y))
    }

    /// Effective visibility: logical AND over the whole ancestor chain.
    #[must_use]
    pub fn world_visible(&self, id: SceneNodeId) -> Option<bool> {
        let mut current = self.nodes.get(&id)?;
        loop {
            if !current.visible {
                return Some(false);
            }
            match current.parent {
                Some(parent) => current = self.nodes.get(&parent)?,
                None => return Some(true),
            }
        }
    }

    /// Effective opacity: product of `0.0..=100.0` opacities along the chain.
    #[must_use]
    pub fn world_opacity(&self, id: SceneNodeId) -> Option<f32> {
        let mut acc = 100.0_f32;
        let mut current = self.nodes.get(&id)?;
        loop {
            acc *= current.opacity / 100.0;
            match current.parent {
                Some(parent) => current = self.nodes.get(&parent)?,
                None => return Some(acc),
            }
        }
    }

    // -- Internes ----------------------------------------------------------

    /// Allocates the next revision (advances `tree_revision`).
    fn alloc(&mut self) -> Revision {
        self.next = self.next.wrapping_add(1);
        Revision(self.next)
    }

    /// Advances the counter without touching any node (root removal).
    fn advance(&mut self) {
        self.next = self.next.wrapping_add(1);
    }

    /// Stamps `id` and all its ancestors with ONE fresh revision.
    /// The node must exist (checked by callers).
    fn touch(&mut self, id: SceneNodeId) -> Revision {
        let revision = self.alloc();
        let mut current = Some(id);
        while let Some(node_id) = current {
            match self.nodes.get_mut(&node_id) {
                Some(node) => {
                    node.revision = revision;
                    current = node.parent;
                }
                None => break,
            }
        }
        revision
    }

    /// Detaches `id` from its parent or roots, with no revision and no
    /// re-insertion (see [`Scene::attach`]/[`Scene::detach`]).
    /// The node must exist (checked by callers).
    fn unlink(&mut self, id: SceneNodeId) {
        let parent = self.nodes.get(&id).and_then(|n| n.parent);
        match parent {
            Some(owner) => {
                if let Some(slot) = self.nodes.get_mut(&owner) {
                    slot.children.retain(|&c| c != id);
                }
            }
            None => self.roots.retain(|&r| r != id),
        }
    }
}
