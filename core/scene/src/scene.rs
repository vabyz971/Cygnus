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

//! Graphe de scène : hiérarchie sémantique, seule source de structure.
//!
//! [`Scene`] possède tous les [`SceneNode`] (table `HashMap` + liste des
//! racines) : c'est elle qui maintient la cohérence `parent`/`children` et
//! alloue les [`Revision`] (voir [`crate::revision`]).
//!
//! Contrat des mutations (uniforme) : elles retournent
//! [`Option<Revision>`] — `None` = cible introuvable ou opération rejetée
//! (cycle, rattachement impossible) ; `Some(revision)` = succès, la
//! révision étant NOUVELLE si quelque chose a changé, INCHANGÉE (valeur
//! courante du nœud) en cas d'écriture sans effet. Les suppressions
//! retournent un `bool` (les nœuds supprimés n'ont plus de révision).
//!
//! Aucune méthode `find_mut` publique : toute mutation passe par `Scene`
//! pour garantir les révisions. Lecture seule via [`Scene::find`].

use std::collections::HashMap;

use math_utils::Transform2D;

use crate::{Affine2, Mask, NodeKind, Revision, SceneNode, SceneNodeId};

/// Hiérarchie sémantique des nœuds visuels d'un document.
///
/// Non_threadée par elle-même (`HashMap` standard) : le partage inter-threads
/// reste à la charge de l'appelant (`Arc`, worker dédié…), comme pour les
/// documents existants de la suite.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    nodes: HashMap<SceneNodeId, SceneNode>,
    roots: Vec<SceneNodeId>,
    next: u64,
}

impl Scene {
    /// Scène vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de nœuds (racines + descendants).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Vrai si la scène ne contient aucun nœud.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Vrai si `id` désigne un nœud de la scène.
    #[must_use]
    pub fn contains(&self, id: SceneNodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Racines dans l'ordre de document.
    #[must_use]
    pub fn roots(&self) -> &[SceneNodeId] {
        &self.roots
    }

    /// Tous les identifiants (ordre non garanti — parcours déterministe via
    /// [`Scene::roots`] + [`Scene::children_of`]).
    #[must_use]
    pub fn all_ids(&self) -> Vec<SceneNodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Dernière révision émise ([`Revision::NONE`] si aucune mutation).
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        Revision(self.next)
    }

    /// Révision courante d'un nœud (`None` si introuvable).
    #[must_use]
    pub fn node_revision(&self, id: SceneNodeId) -> Option<Revision> {
        self.nodes.get(&id).map(|n| n.revision)
    }

    // -- Création ----------------------------------------------------------

    /// Crée un nœud racine (valeurs par défaut : identité, visible,
    /// opacité pleine) et retourne son identifiant frais.
    pub fn create_node(&mut self, kind: NodeKind) -> SceneNodeId {
        let id = SceneNodeId::new();
        let revision = self.alloc();
        self.nodes.insert(id, SceneNode::new(id, kind, revision));
        self.roots.push(id);
        id
    }

    /// Crée un nœud directement comme enfant de `parent`
    /// (`None` si le parent est introuvable).
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

    // -- Lecture -----------------------------------------------------------

    /// Nœud par identifiant (`None` si introuvable).
    #[must_use]
    pub fn find(&self, id: SceneNodeId) -> Option<&SceneNode> {
        self.nodes.get(&id)
    }

    /// Parent direct (`None` si racine ou introuvable — voir
    /// [`Scene::contains`] pour distinguer).
    #[must_use]
    pub fn parent_of(&self, id: SceneNodeId) -> Option<SceneNodeId> {
        self.nodes.get(&id)?.parent
    }

    /// Enfants directs dans l'ordre de document (`None` si introuvable).
    #[must_use]
    pub fn children_of(&self, id: SceneNodeId) -> Option<&[SceneNodeId]> {
        self.nodes.get(&id).map(|n| n.children.as_slice())
    }

    /// Nature sémantique (`None` si introuvable).
    #[must_use]
    pub fn kind_of(&self, id: SceneNodeId) -> Option<&NodeKind> {
        self.nodes.get(&id).map(|n| &n.kind)
    }

    /// Transform local (`None` si introuvable).
    #[must_use]
    pub fn local_transform(&self, id: SceneNodeId) -> Option<Transform2D> {
        self.nodes.get(&id).map(|n| n.local)
    }

    /// Visibilité propre (hors ancêtres — voir [`Scene::world_visible`).
    #[must_use]
    pub fn is_visible(&self, id: SceneNodeId) -> Option<bool> {
        self.nodes.get(&id).map(|n| n.visible)
    }

    /// Opacité propre `0.0..=100.0` (hors ancêtres).
    #[must_use]
    pub fn opacity(&self, id: SceneNodeId) -> Option<f32> {
        self.nodes.get(&id).map(|n| n.opacity)
    }

    /// Masques attachés (`None` si nœud introuvable).
    #[must_use]
    pub fn masks_of(&self, id: SceneNodeId) -> Option<&[Mask]> {
        self.nodes.get(&id).map(|n| n.masks.as_slice())
    }

    /// Métadonnée libre (`None` si nœud ou clé introuvable).
    #[must_use]
    pub fn meta(&self, id: SceneNodeId, key: &str) -> Option<&str> {
        self.nodes.get(&id)?.metadata.get(key).map(String::as_str)
    }

    /// Profondeur (`0` = racine, `None` si introuvable).
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

    /// Taille de la sous-arborescence, nœud inclus (`None` si introuvable).
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

    /// Vrai si `ancestor` est un ancêtre strict de `id`.
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

    // -- Structure ---------------------------------------------------------

    /// Rattache `child` sous `parent`, en fin de fratrie (déplacé depuis
    /// son ancien parent ou ses racines le cas échéant).
    ///
    /// Rejeté (`None`) si un identifiant est introuvable, si
    /// `parent == child`, ou si le rattachement créerait un cycle. Déjà
    /// dernier de la fratrie : sans effet (révision courante retournée).
    /// Même parent mais autre position : déplacé en fin (réordre).
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
        // L'ancienne branche a perdu un sous-arbre : elle change aussi.
        if let Some(old) = old_parent {
            self.touch(old);
        }
        Some(revision)
    }

    /// Détache `child` de son parent (redevient racine, sous-arbre conservé).
    ///
    /// `None` si introuvable, déjà racine, ou… `None` dans les deux cas —
    /// voir [`Scene::contains`] pour distinguer.
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

    /// Supprime `id` et toute sa descendance. Faux si introuvable.
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

    // -- Attributs ---------------------------------------------------------

    /// Remplace le transform local (révision inchangée si valeur identique).
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

    /// Remplace la visibilité (révision inchangée si valeur identique).
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

    /// Remplace l'opacité, bornée à `0.0..=100.0` (révision inchangée si
    /// valeur identique après bornage).
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

    /// Ajoute ou remplace (même `Mask::id`) un masque du nœud.
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

    /// Retire un masque (`Some` sans bump si déjà absent).
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

    /// Pose une métadonnée (révision inchangée si paire identique).
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

    /// Retire une métadonnée (`Some` sans bump si clé absente).
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

    // -- Dérivés monde (jamais stockés) ------------------------------------

    /// Matrice monde : composition des locaux de la racine jusqu'à `id`.
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

    /// Point local de `id` exprimé en coordonnées monde (`None` si inconnu).
    #[must_use]
    pub fn world_point(&self, id: SceneNodeId, x: f32, y: f32) -> Option<(f32, f32)> {
        self.world_affine(id).map(|w| w.apply(x, y))
    }

    /// Visibilité effective : ET logique sur toute la chaîne d'ancêtres.
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

    /// Opacité effective : produit des opacités `0.0..=100.0` sur la chaîne.
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

    /// Alloue la prochaine révision (avance `tree_revision`).
    fn alloc(&mut self) -> Revision {
        self.next = self.next.wrapping_add(1);
        Revision(self.next)
    }

    /// Avance le compteur sans toucher de nœud (suppression de racine).
    fn advance(&mut self) {
        self.next = self.next.wrapping_add(1);
    }

    /// Estampille `id` et tous ses ancêtres avec UNE révision fraîche.
    /// Le nœud doit exister (vérifié par les appelants).
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

    /// Détache `id` de son parent ou des racines, sans révision ni
    /// réinsertion (voir [`Scene::attach`]/[`Scene::detach`]).
    /// Le nœud doit exister (vérifié par les appelants).
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
