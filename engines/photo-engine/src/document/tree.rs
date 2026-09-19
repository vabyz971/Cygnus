use super::compositing::{needs_fallback_in, scope_half_extents};
use super::model::{
    Appearance, BlendMode, FilterLayer, FilterNode, GroupLayer, LayerMask, LayerNode, PixelLayer,
    RgbaBuf, Transform2D,
};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use std::cell::RefCell;
use std::sync::Arc;
use uuid::Uuid;

pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Pile racine — index 0 = BAS de la pile (premier dessiné)
    pub root: Vec<LayerNode>,
    /// Cache d'apparences par calque ([`crate::renderer::Renderer`]) :
    /// validité par signature de filtres + identité de source, alimenté
    /// par la chaîne GPU compute / CPU rayon. Interior mutability car le
    /// cache est un détail de performance invisible depuis l'API (&self).
    cache: RefCell<crate::renderer::Renderer>,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            root: Vec::new(),
            cache: RefCell::new(crate::renderer::Renderer::default()),
        }
    }

    /// Document vierge avec un premier calque transparent aux
    /// dimensions du document (« Créer un document » sans image).
    pub fn with_blank_layer(width: u32, height: u32) -> Self {
        let mut doc = Self::new(width.max(1), height.max(1));
        let blank = DynamicImage::new_rgba8(doc.width, doc.height);
        doc.push_layer(LayerNode::Pixel(PixelLayer::new(
            "Calque 1",
            Arc::new(blank),
        )));
        doc
    }

    /// Reconstruit le document depuis un état restauré (undo/redo, projet).
    /// Le cache d'apparence est vidé : les entrées restaurées se
    /// revalideront par signature à la première demande.
    pub fn restore(&mut self, width: u32, height: u32, root: Vec<LayerNode>) {
        self.width = width;
        self.height = height;
        self.root = root;
        self.cache.borrow_mut().invalidate_all();
    }

    // -- Recherche ----------------------------------------------------------

    pub fn find(&self, id: Uuid) -> Option<&LayerNode> {
        find_in(&self.root, id)
    }

    pub fn find_mut(&mut self, id: Uuid) -> Option<&mut LayerNode> {
        find_in_mut(&mut self.root, id)
    }

    /// Accès typé au calque pixels.
    pub fn pixel_layer(&self, id: Uuid) -> Option<&PixelLayer> {
        match self.find(id) {
            Some(LayerNode::Pixel(l)) => Some(l),
            _ => None,
        }
    }

    pub fn pixel_layer_mut(&mut self, id: Uuid) -> Option<&mut PixelLayer> {
        match self.find_mut(id) {
            Some(LayerNode::Pixel(l)) => Some(l),
            _ => None,
        }
    }

    /// Liste plate des calques pixels, ordre de dessin (bas → haut), DFS.
    #[must_use]
    pub fn iter_pixels(&self) -> Vec<&PixelLayer> {
        let mut out = Vec::with_capacity(self.root.len());
        collect_pixels(&self.root, &mut out);
        out
    }

    /// Ids de TOUS les calques pixels (visibles ou non, groupes inclus),
    /// ordre DFS. Sert au partage inter-passes : le snapshot a besoin des
    /// miniatures même des calques masqués (contrairement à
    /// [`Self::iter_pixels`] qui ne retient que le contribuant).
    #[must_use]
    pub fn all_pixel_ids(&self) -> Vec<Uuid> {
        let mut out = Vec::new();
        collect_all_pixel_ids(&self.root, &mut out);
        out
    }

    pub fn pixel_count(&self) -> usize {
        self.iter_pixels().len()
    }

    /// Liste plate de tous les masques de l'arbre, chacun avec l'id du calque
    /// porteur (calques pixels ET groupes). Ordre : DFS, bas → haut.
    #[must_use]
    pub fn iter_masks(&self) -> Vec<(Uuid, &crate::document::LayerMask)> {
        let mut out = Vec::new();
        collect_masks(&self.root, &mut out);
        out
    }

    /// Le rendu rapide « 1 texture par calque » est-il possible, ou faut-il
    /// passer par la composite CPU ? Vrai dès qu'un groupe a un mode de
    /// fusion non-Normal ou qu'un calque d'ajustement agit sur la pile.
    pub fn needs_fallback(&self) -> bool {
        needs_fallback_in(&self.root)
    }

    // -- Mutations structurelles ---------------------------------------------

    /// Ajoute un nœud au SOMMET de la pile racine.
    pub fn push_layer(&mut self, node: LayerNode) {
        self.root.push(node);
    }

    /// Insère `node` juste au-dessus de `anchor` (même parent).
    pub fn insert_above(&mut self, anchor: Uuid, node: LayerNode) -> bool {
        match find_owner_list(&mut self.root, anchor) {
            Some((list, idx)) => {
                list.insert(idx + 1, node);
                true
            }
            None => false,
        }
    }

    /// Détache le sous-arbre `id` de son parent.
    pub fn remove(&mut self, id: Uuid) -> Option<LayerNode> {
        let (list, idx) = find_owner_list(&mut self.root, id)?;
        Some(list.remove(idx))
    }

    /// Duplique le sous-arbre (nouveaux ids partout) et l'insère au-dessus.
    pub fn duplicate(&mut self, id: Uuid) -> Option<Uuid> {
        let mut copy = self.find(id)?.clone();
        copy.regenerate_ids();
        let new_id = copy.id();
        if !self.insert_above(id, copy) {
            return None;
        }
        Some(new_id)
    }

    /// Monte d'un cran parmi les frères (vers le haut de la pile).
    pub fn move_up(&mut self, id: Uuid) -> bool {
        let Some((list, idx)) = find_owner_list(&mut self.root, id) else {
            return false;
        };
        if idx + 1 >= list.len() {
            return false;
        }
        list.swap(idx, idx + 1);
        true
    }

    /// Descend d'un cran parmi les frères.
    pub fn move_down(&mut self, id: Uuid) -> bool {
        let Some((list, idx)) = find_owner_list(&mut self.root, id) else {
            return false;
        };
        if idx == 0 {
            return false;
        }
        list.swap(idx, idx - 1);
        true
    }

    /// Réordonne par drag & drop : déplace `dragged` avant ou après `target`.
    pub fn reorder_before(&mut self, dragged: Uuid, target: Uuid, before: bool) -> bool {
        if !self.can_reorder_before(dragged, target)
            || self.is_noop_reorder(dragged, target, before)
        {
            return false;
        }
        let Some(node) = self.remove(dragged) else {
            return false;
        };
        let Some((list, idx)) = find_owner_list(&mut self.root, target) else {
            // target disparu ? restaure à la fin
            self.push_layer(node);
            return false;
        };
        let at = if before { idx } else { idx + 1 };
        let at = at.min(list.len());
        list.insert(at, node);
        true
    }

    /// Le déplacement avant/après est-il structurellement valide ?
    ///
    /// Un déplacement adjacent sans effet reste une cible valide : il sera
    /// simplement ignoré au commit, sans entrée d'historique.
    #[must_use]
    pub fn can_reorder_before(&self, dragged: Uuid, target: Uuid) -> bool {
        if dragged == target {
            return false;
        }
        if self.find(dragged).is_none() || self.find(target).is_none() {
            return false;
        }
        // Empêche de déplacer un groupe dans son propre sous-arbre
        if let Some(LayerNode::Group(groupe)) = self.find(dragged)
            && Self::contains_id(&groupe.children, target)
        {
            return false;
        }
        true
    }

    /// Déplace `dragged` en tête des enfants de `group`.
    pub fn move_into(&mut self, dragged: Uuid, group: Uuid) -> bool {
        if !self.can_move_into(dragged, group) || self.is_noop_move_into(dragged, group) {
            return false;
        }
        let Some(node) = self.remove(dragged) else {
            return false;
        };
        let Some(LayerNode::Group(groupe)) = self.find_mut(group) else {
            // groupe disparu ? restaure à la fin
            self.push_layer(node);
            return false;
        };
        groupe.children.insert(0, node);
        true
    }

    /// L'insertion en tête de groupe est-elle structurellement valide ?
    #[must_use]
    pub fn can_move_into(&self, dragged: Uuid, group: Uuid) -> bool {
        if dragged == group {
            return false;
        }
        if self.find(dragged).is_none() {
            return false;
        }
        let Some(LayerNode::Group(_groupe)) = self.find(group) else {
            return false;
        };
        if let Some(LayerNode::Group(dragged_groupe)) = self.find(dragged)
            && Self::contains_id(&dragged_groupe.children, group)
        {
            return false;
        }
        true
    }

    fn is_noop_reorder(&self, dragged: Uuid, target: Uuid, before: bool) -> bool {
        let (Some((drag_parent, drag_idx)), Some((target_parent, target_idx))) = (
            owner_position(&self.root, dragged, None),
            owner_position(&self.root, target, None),
        ) else {
            return false;
        };
        if drag_parent != target_parent {
            return false;
        }
        if before {
            drag_idx.saturating_add(1) == target_idx
        } else {
            drag_idx == target_idx.saturating_add(1)
        }
    }

    fn is_noop_move_into(&self, dragged: Uuid, group: Uuid) -> bool {
        match self.find(group) {
            Some(LayerNode::Group(groupe)) => groupe
                .children
                .first()
                .is_some_and(|enfant| enfant.id() == dragged),
            _ => false,
        }
    }

    fn contains_id(nodes: &[LayerNode], id: Uuid) -> bool {
        for n in nodes {
            if n.id() == id {
                return true;
            }
            if let LayerNode::Group(g) = n
                && Self::contains_id(&g.children, id)
            {
                return true;
            }
        }
        false
    }

    /// Regroupe les nœuds donnés (mêmes frères) dans un nouveau groupe
    /// inséré à la place du plus bas d'entre eux. L'ordre relatif de la
    /// pile est préservé. Retourne l'id du groupe créé.
    pub fn group(&mut self, ids: &[Uuid]) -> Option<Uuid> {
        let first = *ids.first()?;
        let (list, _) = find_owner_list(&mut self.root, first)?;
        // Positions une fois pour toutes, triées (ordre pile)
        let mut idxs: Vec<usize> = Vec::with_capacity(ids.len());
        for id in ids {
            idxs.push(list.iter().position(|n| n.id() == *id)?);
        }
        idxs.sort_unstable();
        // Extraction du plus haut vers le plus bas, puis remise en ordre
        let mut children: Vec<LayerNode> = Vec::with_capacity(idxs.len());
        for &i in idxs.iter().rev() {
            children.push(list.remove(i));
        }
        children.reverse();
        let group = LayerNode::Group(GroupLayer::new("Groupe", children));
        let gid = group.id();
        let at = (*idxs.first()?).min(list.len());
        list.insert(at, group);
        Some(gid)
    }

    /// Dissout un groupe : ses enfants remontent à sa place dans le parent.
    /// Retourne les ids des enfants libérés.
    pub fn ungroup(&mut self, id: Uuid) -> Option<Vec<Uuid>> {
        let (list, idx) = find_owner_list(&mut self.root, id)?;
        if !matches!(list.get(idx), Some(LayerNode::Group(_))) {
            return None;
        }
        let node = list.remove(idx);
        let LayerNode::Group(group) = node else {
            return None;
        };
        let child_ids: Vec<Uuid> = group.children.iter().map(LayerNode::id).collect();
        for (off, child) in group.children.into_iter().enumerate() {
            list.insert(idx + off, child);
        }
        Some(child_ids)
    }

    // -- Éditions destructives (pixels) ---------------------------------------

    /// Retourne le calque horizontalement/verticalement (destructif).
    ///
    /// # Errors
    /// Retourne une erreur si le calque n'existe pas.
    pub fn flip(&mut self, id: Uuid, horizontal: bool) -> Result<(), String> {
        let layer = self.pixel_layer_mut(id).ok_or("calque introuvable")?;
        let flipped = if horizontal {
            layer.source_image.fliph()
        } else {
            layer.source_image.flipv()
        };
        for mask in &mut layer.masks {
            let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
            let flipped_mask = if horizontal {
                dyn_mask.fliph()
            } else {
                dyn_mask.flipv()
            }
            .to_rgba8();
            mask.image = Arc::new(flipped_mask);
            mask.touch();
        }
        // Les masques des sous-calques vivent dans le même espace source
        for f in &mut layer.filter_layers {
            for mask in &mut f.masks {
                let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
                let flipped_mask = if horizontal {
                    dyn_mask.fliph()
                } else {
                    dyn_mask.flipv()
                }
                .to_rgba8();
                mask.image = Arc::new(flipped_mask);
                mask.touch();
            }
        }
        layer.set_source_image(flipped);
        Ok(())
    }

    /// Rogne le calque au rect (coordonnées CALQUE, pixels). Destructif :
    /// le contenu reste en place dans le monde (le transform compense
    /// l'origine du crop). Erreur descriptive si le rect est invalide.
    ///
    /// # Errors
    /// Retourne une erreur si le calque est introuvable ou si le rectangle dépasse les bords.
    pub fn crop(&mut self, id: Uuid, x: i32, y: i32, w: u32, h: u32) -> Result<(), String> {
        let layer = self.pixel_layer_mut(id).ok_or("calque introuvable")?;
        let (iw, ih) = layer.dimensions();
        if w == 0 || h == 0 {
            return Err("rogner : dimensions nulles".into());
        }
        if x < 0 || y < 0 || x + w as i32 > iw as i32 || y + h as i32 > ih as i32 {
            return Err("rogner : la sélection dépasse les bords du calque".into());
        }
        let cropped = layer.source_image.crop_imm(x as u32, y as u32, w, h);
        // Compense l'origine : le pixel (x,y) d'origine reste à sa place monde
        layer.transform.offset_x += x as f32;
        layer.transform.offset_y += y as f32;
        // Rogne les masques liés aux mêmes coordonnées
        for mask in &mut layer.masks {
            let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
            let cropped_mask = dyn_mask.crop_imm(x as u32, y as u32, w, h).to_rgba8();
            mask.image = Arc::new(cropped_mask);
            mask.touch();
        }
        // Idem pour les masques des sous-calques (même espace source)
        for f in &mut layer.filter_layers {
            for mask in &mut f.masks {
                let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
                let cropped_mask = dyn_mask.crop_imm(x as u32, y as u32, w, h).to_rgba8();
                mask.image = Arc::new(cropped_mask);
                mask.touch();
            }
        }
        layer.set_source_image(cropped);
        Ok(())
    }

    /// Remplace l'image source d'un calque pixels (peinture…).
    pub fn set_source_image(&mut self, id: Uuid, image: DynamicImage) -> bool {
        match self.pixel_layer_mut(id) {
            Some(layer) => {
                layer.set_source_image(image);
                true
            }
            None => false,
        }
    }

    // -- Filtres dynamiques ----------------------------------------------------

    /// Ajoute un sous-calque de filtre en fin de chaîne d'un calque pixels.
    /// Pour un ajustement, le sous-calque est converti en filtre simple
    /// (les ajustements portent opacité/fusion au niveau du calque).
    /// Retourne l'id du filtre inséré.
    pub fn add_filter(&mut self, layer_id: Uuid, filter: FilterLayer) -> Option<Uuid> {
        let fid = filter.id;
        match self.find_mut(layer_id)? {
            LayerNode::Pixel(l) => l.filter_layers.push(filter),
            LayerNode::Adjustment(a) => a.filters.push(FilterNode {
                id: filter.id,
                type_id: filter.type_id,
                params: filter.params,
                enabled: filter.enabled,
            }),
            LayerNode::Group(_) => return None,
        }
        self.touch_pixel(layer_id);
        Some(fid)
    }

    /// Retire un sous-calque de filtre (pixels) ou un filtre d'ajustement.
    pub fn remove_filter(&mut self, layer_id: Uuid, filter_id: Uuid) -> Option<FilterLayer> {
        let removed = match self.find_mut(layer_id)? {
            LayerNode::Pixel(l) => {
                let idx = l.filter_layers.iter().position(|f| f.id == filter_id)?;
                l.filter_layers.remove(idx)
            }
            LayerNode::Adjustment(a) => {
                let idx = a.filters.iter().position(|f| f.id == filter_id)?;
                let f = a.filters.remove(idx);
                FilterLayer::from_node(f)
            }
            LayerNode::Group(_) => return None,
        };
        self.touch_pixel(layer_id);
        Some(removed)
    }

    /// Modifie un paramètre de filtre (geste continu : coalescence côté app).
    pub fn set_filter_param(
        &mut self,
        layer_id: Uuid,
        filter_id: Uuid,
        key: impl Into<String>,
        value: datatypes::ParamValue,
    ) -> bool {
        let key = key.into();
        let params = match self.find_mut(layer_id) {
            Some(LayerNode::Pixel(l)) => l
                .filter_layers
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.params),
            Some(LayerNode::Adjustment(a)) => a
                .filters
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.params),
            _ => None,
        };
        let Some(params) = params else {
            return false;
        };
        params.insert(key, value);
        self.touch_pixel(layer_id);
        true
    }

    /// Active/désactive un filtre sans perdre ses réglages.
    pub fn set_filter_enabled(&mut self, layer_id: Uuid, filter_id: Uuid, enabled: bool) -> bool {
        let target = match self.find_mut(layer_id) {
            Some(LayerNode::Pixel(l)) => l
                .filter_layers
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.enabled),
            Some(LayerNode::Adjustment(a)) => a
                .filters
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.enabled),
            _ => None,
        };
        let Some(slot) = target else {
            return false;
        };
        if *slot != enabled {
            *slot = enabled;
            self.touch_pixel(layer_id);
        }
        true
    }

    /// Déplace un sous-calque de filtre dans la chaîne de son parent
    /// (ordre = ordre d'application, façon Affinity).
    pub fn move_filter(&mut self, layer_id: Uuid, filter_id: Uuid, up: bool) -> bool {
        let Some(LayerNode::Pixel(l)) = self.find_mut(layer_id) else {
            return false;
        };
        let Some(idx) = l.filter_layers.iter().position(|f| f.id == filter_id) else {
            return false;
        };
        let other = if up {
            idx.checked_add(1)
        } else {
            idx.checked_sub(1)
        };
        let Some(other) = other else {
            return false;
        };
        if other >= l.filter_layers.len() {
            return false;
        }
        l.filter_layers.swap(idx, other);
        l.touch();
        true
    }

    /// Duplique un sous-calque de filtre (nouvel id) juste au-dessus.
    pub fn duplicate_filter(&mut self, layer_id: Uuid, filter_id: Uuid) -> Option<Uuid> {
        let Some(LayerNode::Pixel(l)) = self.find_mut(layer_id) else {
            return None;
        };
        let idx = l.filter_layers.iter().position(|f| f.id == filter_id)?;
        let mut copy = l.filter_layers[idx].clone();
        copy.id = Uuid::new_v4();
        // Les masques sont adressés par id (cache de miniatures) : ids frais.
        for m in &mut copy.masks {
            m.id = Uuid::new_v4();
        }
        let new_id = copy.id;
        l.filter_layers.insert(idx + 1, copy);
        l.touch();
        Some(new_id)
    }

    fn touch_pixel(&mut self, layer_id: Uuid) {
        if let Some(LayerNode::Pixel(l)) = self.find_mut(layer_id) {
            l.touch();
        }
    }

    // -- Recherche de sous-calques -------------------------------------------

    /// Sous-calque de filtre par son id (n'importe quel calque pixels,
    /// racine ou groupe imbriqué).
    pub fn find_filter_layer(&self, filter_id: Uuid) -> Option<&FilterLayer> {
        find_filter_in(&self.root, filter_id)
    }

    pub fn find_filter_layer_mut(&mut self, filter_id: Uuid) -> Option<&mut FilterLayer> {
        find_filter_in_mut(&mut self.root, filter_id)
    }

    /// Calque pixels porteur d'un sous-calque de filtre.
    pub fn find_filter_parent(&self, filter_id: Uuid) -> Option<Uuid> {
        find_filter_parent_in(&self.root, filter_id)
    }

    // -- Attributs polymorphes (nœud OU sous-calque de filtre) ---------------

    /// Transform du nœud (pixels) ou du sous-calque de filtre.
    pub fn transform_of(&self, id: Uuid) -> Option<Transform2D> {
        match self.find(id) {
            Some(LayerNode::Pixel(l)) => Some(l.transform),
            _ => self.find_filter_layer(id).map(|f| f.transform),
        }
    }

    /// Remplace la transform (touche le calque porteur pour l'invalidation).
    pub fn set_transform_any(&mut self, id: Uuid, transform: Transform2D) -> bool {
        if let Some(node) = self.find_mut(id) {
            if let LayerNode::Pixel(l) = node {
                l.transform = transform;
                return true;
            }
            return false;
        }
        let parent = self.find_filter_parent(id);
        if let (Some(pid), Some(f)) = (parent, self.find_filter_layer_mut(id)) {
            f.transform = transform;
            self.touch_pixel(pid);
            return true;
        }
        false
    }

    /// Opacité 0..=100 du nœud ou du sous-calque de filtre.
    pub fn opacity_of(&self, id: Uuid) -> Option<f32> {
        match self.find(id) {
            Some(n) => Some(n.opacity()),
            None => self.find_filter_layer(id).map(|f| f.opacity),
        }
    }

    pub fn set_opacity_any(&mut self, id: Uuid, opacity: f32) -> bool {
        if let Some(node) = self.find_mut(id) {
            node.set_opacity(opacity);
            return true;
        }
        let parent = self.find_filter_parent(id);
        if let (Some(pid), Some(f)) = (parent, self.find_filter_layer_mut(id)) {
            f.opacity = opacity.clamp(0.0, 100.0);
            self.touch_pixel(pid);
            return true;
        }
        false
    }

    /// Mode de fusion du nœud ou du sous-calque de filtre.
    pub fn blend_of(&self, id: Uuid) -> Option<BlendMode> {
        match self.find(id) {
            Some(n) => n.blend_mode(),
            None => self.find_filter_layer(id).map(|f| f.blend_mode),
        }
    }

    pub fn set_blend_any(&mut self, id: Uuid, mode: BlendMode) -> bool {
        if let Some(node) = self.find_mut(id) {
            node.set_blend_mode(mode);
            return true;
        }
        let parent = self.find_filter_parent(id);
        if let (Some(pid), Some(f)) = (parent, self.find_filter_layer_mut(id)) {
            f.blend_mode = mode;
            self.touch_pixel(pid);
            return true;
        }
        false
    }

    /// Renomme un nœud, un sous-calque de filtre ou un masque.
    pub fn set_name_any(&mut self, id: Uuid, name: String) -> bool {
        if let Some(node) = self.find_mut(id) {
            node.set_name(name);
            return true;
        }
        if let Some(pid) = self.find_filter_parent(id)
            && let Some(f) = self.find_filter_layer_mut(id)
        {
            f.name = name;
            self.touch_pixel(pid);
            return true;
        }
        let Some(owner) = self.mask_owner_of(id) else {
            return false;
        };
        let Some(masks) = self.masks_of_mut(owner) else {
            return false;
        };
        if let Some(slot) = masks.iter_mut().find(|m| m.id == id) {
            slot.name = name;
            return true;
        }
        false
    }

    /// Porteur d'un masque (nœud ou sous-calque de filtre) — `None` sinon.
    pub fn mask_owner_of(&self, mask_id: Uuid) -> Option<Uuid> {
        mask_owner_in(&self.root, mask_id)
    }

    /// Nom d'affichage d'un masque par son id.
    pub fn mask_name(&self, mask_id: Uuid) -> Option<String> {
        let owner = self.mask_owner_of(mask_id)?;
        self.masks_of(owner)?
            .iter()
            .find(|m| m.id == mask_id)
            .map(|m| m.name.clone())
    }

    /// Déplace un masque dans la liste de son porteur. `up = true` → vers
    /// le HALT de pile affiché (index décroissant, liste des masques = ordre
    /// d'affichage). Retourne `false` si aux limites ou introuvable.
    pub fn move_mask(&mut self, owner_id: Uuid, mask_id: Uuid, up: bool) -> bool {
        let Some(masks) = self.masks_of_mut(owner_id) else {
            return false;
        };
        let Some(idx) = masks.iter().position(|m| m.id == mask_id) else {
            return false;
        };
        let other = if up {
            idx.checked_sub(1)
        } else {
            idx.checked_add(1)
        };
        let Some(other) = other else {
            return false;
        };
        if other >= masks.len() {
            return false;
        }
        masks.swap(idx, other);
        true
    }

    // -- Masques (nœuds ET sous-calques de filtre) ----------------------------

    /// Masque par (porteur, id) — le porteur est un nœud ou un sous-calque.
    pub fn mask_of(&self, owner_id: Uuid, mask_id: Uuid) -> Option<&LayerMask> {
        self.masks_of(owner_id)?.iter().find(|m| m.id == mask_id)
    }

    pub fn mask_of_mut(&mut self, owner_id: Uuid, mask_id: Uuid) -> Option<&mut LayerMask> {
        self.masks_of_mut(owner_id)?
            .iter_mut()
            .find(|m| m.id == mask_id)
    }

    /// Tous les masques d'un porteur (nœud ou sous-calque de filtre).
    pub fn masks_of(&self, owner_id: Uuid) -> Option<&[LayerMask]> {
        match self.find(owner_id) {
            Some(n) => Some(n.masks()),
            None => self.find_filter_layer(owner_id).map(|f| f.masks.as_slice()),
        }
    }

    pub fn masks_of_mut(&mut self, owner_id: Uuid) -> Option<&mut Vec<LayerMask>> {
        if self.find(owner_id).is_some() {
            return self.find_mut(owner_id)?.masks_mut();
        }
        self.find_filter_layer_mut(owner_id).map(|f| &mut f.masks)
    }

    // -- Commandes d'historique ------------------------------------------------

    /// Applique `command.new` au document et retourne l'INVERSE
    /// (old/new échangés) prêt à empiler pour le redo.
    ///
    /// Routage systématique par les setters existants : les invariants du
    /// modèle sont préservés (clamp d'opacité, bump de version d'apparence
    /// pour l'invalidation ciblée du cache, clamp de scale).
    ///
    /// Si le nœud cible a disparu, la commande est retournée telle quelle :
    /// l'empiler reste sûr (réapplication = no-op).
    pub fn apply_command(&mut self, command: crate::command::Command) -> crate::command::Command {
        use crate::command::Command;
        match command {
            Command::SetOpacity { layer_id, old, new } => {
                self.set_opacity_any(layer_id, new);
                Command::SetOpacity {
                    layer_id,
                    old: new,
                    new: old,
                }
            }
            Command::SetTransform { layer_id, old, new } => {
                self.set_transform_any(layer_id, new);
                Command::SetTransform {
                    layer_id,
                    old: new,
                    new: old,
                }
            }
            Command::SetBlendMode { node_id, old, new } => {
                self.set_blend_any(node_id, new);
                Command::SetBlendMode {
                    node_id,
                    old: new,
                    new: old,
                }
            }
            Command::SetVisibility { node_id, old, new } => {
                if let Some(node) = self.find_mut(node_id) {
                    node.set_visible(new);
                }
                Command::SetVisibility {
                    node_id,
                    old: new,
                    new: old,
                }
            }
            Command::SetFilterParam {
                layer_id,
                filter_id,
                param_name,
                old,
                new,
            } => {
                self.set_filter_param(layer_id, filter_id, param_name.clone(), new.clone());
                Command::SetFilterParam {
                    layer_id,
                    filter_id,
                    param_name,
                    old: new,
                    new: old,
                }
            }
            Command::RenameLayer { node_id, old, new } => {
                self.set_name_any(node_id, new.clone());
                Command::RenameLayer {
                    node_id,
                    old: new,
                    new: old,
                }
            }
            Command::SetMaskEnabled {
                node_id,
                mask_id,
                old,
                new,
            } => {
                if let Some(mask) = self.mask_of_mut(node_id, mask_id) {
                    mask.enabled = new;
                    mask.touch();
                }
                Command::SetMaskEnabled {
                    node_id,
                    mask_id,
                    old: new,
                    new: old,
                }
            }
            Command::SetMaskInverted {
                node_id,
                mask_id,
                old,
                new,
            } => {
                if let Some(mask) = self.mask_of_mut(node_id, mask_id) {
                    mask.inverted = new;
                    mask.touch();
                }
                Command::SetMaskInverted {
                    node_id,
                    mask_id,
                    old: new,
                    new: old,
                }
            }
        }
    }

    // -- Apparence ------------------------------------------------------------

    /// Apparence dérivée du calque (source × filtres actifs), servie par
    /// le [`crate::renderer::Renderer`] : HIT = zéro recalcul, MISS =
    /// exécution de la chaîne (compute shaders si GPU disponible).
    /// Retourne des clones bon marché (Arc/RgbaBuf).
    pub fn appearance(&self, id: Uuid) -> Option<Appearance> {
        let layer = self.pixel_layer(id)?;
        Some(self.cache.borrow_mut().appearance(layer))
    }

    /// Variante [`Self::appearance`] sans AUCUNE exécution : retourne
    /// l'apparence si le cache la détient encore valide, `None` sinon.
    /// Utilisée par la synchronisation UI à chaque message — à chaud, elle
    /// ne touche ni au pool de rendu ni à la chaîne de filtres.
    pub fn appearance_hit(&self, id: Uuid) -> Option<Appearance> {
        let layer = self.pixel_layer(id)?;
        self.cache.borrow_mut().appearance_hit(layer)
    }

    /// Image seule (chemin compositing — évite de régénérer preview/thumb).
    pub fn appearance_image(&self, id: Uuid) -> Option<Arc<DynamicImage>> {
        self.appearance(id).map(|a| a.image)
    }

    /// Exporte l'entrée d'apparence chaude d'un calque pixels (transfert
    /// vers le document vivant après calcul en `spawn_blocking` — voir
    /// [`crate::renderer::WarmedAppearance`]).
    pub fn export_warmed(&self, id: Uuid) -> Option<crate::renderer::WarmedAppearance> {
        self.cache.borrow().export_warmed(id)
    }

    /// Insère une entrée pré-calculée hors thread UI : le prochain
    /// `appearance_hit` HIT sans exécuter la chaîne sur l'UI.
    pub fn insert_warmed(&mut self, id: Uuid, warmed: crate::renderer::WarmedAppearance) {
        self.cache.borrow_mut().insert_warmed(id, warmed);
    }

    /// Miniature pour le panneau Calques (apparence dérivée).
    pub fn thumb(&self, id: Uuid) -> Option<RgbaBuf> {
        self.appearance(id).map(|a| a.thumb)
    }

    /// Réchauffe le cache d'apparences de CE document à partir des entrées
    /// actuellement chaudes d'un autre document (typiquement le document
    /// VIVANT, pour un clone envoyé en tâche de fond). Évite de re-exécuter
    /// les chaînes de rendu (filtres preview/thumb) à chaque composite de
    /// fond : les calques inchangés HIT au lieu de MISS.
    pub fn warm_cache_from(&mut self, other: &Document) {
        if std::ptr::eq(self, other) {
            return;
        }
        let mut dst = self.cache.borrow_mut();
        let src = other.cache.borrow();
        dst.import_from(&src);
    }

    // -- Historique -------------------------------------------------------------

    /// Instantané complet (pixels partagés par Arc — quasi gratuit).
    pub fn snapshot(&self) -> crate::history::Snapshot {
        crate::history::Snapshot {
            doc_size: (self.width, self.height),
            root: self.root.clone(),
        }
    }

    /// Restaure un instantané.
    pub fn restore_snapshot(&mut self, snap: crate::history::Snapshot) {
        self.restore(snap.doc_size.0, snap.doc_size.1, snap.root);
    }

    // -- Compositing ---------------------------------------------------------------

    /// Composite pour le plan de travail infini : aucun crop au document.
    /// Le document reste centré (comme Affinity/Photoshop) et les calques
    /// hors document restent visibles. Retourne None si rien n'est visible.
    pub fn composite_preview(&self) -> Option<DynamicImage> {
        self.composite_scope(&self.root)
    }

    /// Composite du plan infini SANS le sous-arbre donné — utilisé pour le
    /// fond pré-calculé pendant un drag. Réutilise le CACHE D'APPARENCES de
    /// CE document : tous les calques restants produisent des HIT, le coût
    /// se limite au blend lui-même (contrairement à un clonage dans un
    /// document neuf dont le cache est froid).
    pub fn composite_preview_without(&self, exclude_id: Uuid) -> Option<DynamicImage> {
        // Clone structurel bon marché (Arcs partagés), sous-arbre masqué,
        // puis composite via LE MÊME cache que le document vivant.
        let mut hidden = self.root.clone();
        hide_subtree(find_in_mut(&mut hidden, exclude_id));
        if hidden.is_empty() {
            return None;
        }
        self.composite_scope(&hidden)
    }

    /// Échantillonne la couleur compositée à un point donné en coordonnées
    /// document locales (0,0 = coin haut-gauche du DOCUMENT). Retourne
    /// `[R,G,B,A]` ou `None` si hors du plan composite ou document vide.
    /// Conçu pour être appelé UNIQUEMENT dans `spawn_blocking` — effectue
    /// un composite complet si nécessaire.
    pub fn sample_color(&self, dx: f32, dy: f32) -> Option<[u8; 4]> {
        // Mêmes géométrie et origine que `composite_scope` : si on les
        // redérivait des dimensions ARRONDIES du buffer, la correspondance
        // serait fausse dès que les extents sont fractionnaires.
        let resolver = |id: Uuid| self.appearance_image(id);
        let (half_w, half_h) = scope_half_extents(&self.root, self.width, self.height, &resolver);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        // La composite place les pixels à l'index « plancher » du document
        // (blend_into arrondit et pose à `offset` exactement) : le pixel qui
        // occupe la case doc [p, p+1) vit à l'index p — floor(), pas round().
        let px = (origin_x + dx).floor() as i64;
        let py = (origin_y + dy).floor() as i64;
        if px < 0 || py < 0 || px >= i64::from(w) || py >= i64::from(h) {
            return None;
        }
        let img = self.composite_preview()?;
        let p = img.get_pixel(px as u32, py as u32);
        Some([p[0], p[1], p[2], p[3]])
    }

    /// Échantillonne un PATCH carré `side`×`side` (RGBA8, lignes majeures)
    /// centré sur le point document `(dx, dy)` — pixels hors plan composite
    /// transparents. Alimente la LOUPE de la pipette (grossissement façon
    /// Photoshop). Même composite complète que [`Self::sample_color`] →
    /// à appeler UNIQUEMENT dans `spawn_blocking`.
    pub fn sample_region(&self, dx: f32, dy: f32, side: u32) -> Option<Vec<u8>> {
        let resolver = |id: Uuid| self.appearance_image(id);
        let (half_w, half_h) = scope_half_extents(&self.root, self.width, self.height, &resolver);
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        let px = (origin_x + dx).floor() as i64;
        let py = (origin_y + dy).floor() as i64;
        let img = self.composite_preview()?;
        let (iw, ih) = img.dimensions();
        let side = side.max(1);
        let half = i64::from(side / 2);
        let x0 = px - half;
        let y0 = py - half;
        let mut out = vec![0u8; (side * side * 4) as usize];
        for ry in 0..i64::from(side) {
            for rx in 0..i64::from(side) {
                let gx = x0 + rx;
                let gy = y0 + ry;
                let sample = if gx >= 0 && gy >= 0 && gx < i64::from(iw) && gy < i64::from(ih) {
                    let p = img.get_pixel(gx as u32, gy as u32);
                    [p[0], p[1], p[2], p[3]]
                } else {
                    [0, 0, 0, 0]
                };
                let o = (ry as usize * side as usize + rx as usize) * 4;
                out[o..o + 4].copy_from_slice(&sample);
            }
        }
        Some(out)
    }

    /// Géométrie du composite du plan infini : dimensions pleine
    /// résolution `(w, h)` et coin haut-gauche du DOCUMENT `(ox, oy)`
    /// dans ces pixels. `None` si rien n'est visible. Les demi-extents
    /// sont symétriques par construction (`scope_half_extents`) : le
    /// document est donc toujours centré dans le buffer.
    /// Partagée avec l'UI (aperçu miniature) pour convertir
    /// écran→pixels document sans dérive.
    pub fn preview_geometry(&self) -> Option<(u32, u32, f32, f32)> {
        self.preview_geometry_of(&self.root)
    }

    /// Variante de [`Self::preview_geometry`] avec résolveur externe :
    /// partage les apparences déjà résolues pendant la réponse (zéro
    /// nouvelle résolution). Comportement identique à parité de
    /// résolveur.
    pub fn preview_geometry_with(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<(u32, u32, f32, f32)> {
        self.preview_geometry_of_with(&self.root, resolve)
    }

    /// Variante de [`Self::preview_geometry`] sur une portée donnée.
    fn preview_geometry_of(&self, nodes: &[LayerNode]) -> Option<(u32, u32, f32, f32)> {
        let resolver = |id: Uuid| self.appearance_image(id);
        self.preview_geometry_of_with(nodes, &resolver)
    }

    /// Corps de [`Self::preview_geometry_of`] à résolveur injecté.
    fn preview_geometry_of_with(
        &self,
        nodes: &[LayerNode],
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<(u32, u32, f32, f32)> {
        let (half_w, half_h) = scope_half_extents(nodes, self.width, self.height, resolve);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        // Coût nul si rien ne contribue (évite un composite fantôme).
        if !contributes(nodes, resolve) {
            return None;
        }
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        Some((w.max(1), h.max(1), origin_x, origin_y))
    }

    fn composite_scope(&self, nodes: &[LayerNode]) -> Option<DynamicImage> {
        let resolver = |id: Uuid| self.appearance_image(id);
        let mut stats = super::compositing::CompositeStats::default();
        self.composite_scope_with(nodes, &resolver, &mut stats)
    }

    /// Composite du plan infini avec résolveur externe : partage les
    /// apparences déjà résolues pendant la réponse (zéro nouvelle
    /// résolution). Comportement identique à parité de résolveur.
    pub fn composite_preview_with(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<DynamicImage> {
        let mut stats = super::compositing::CompositeStats::default();
        self.composite_scope_with(&self.root, resolve, &mut stats)
    }

    /// Variante instrumentée de [`Self::composite_preview_with`] : remplit
    /// `stats` (scope, alloc accumulateur, blends, pixels parcourus).
    /// Comportement de rendu identique.
    pub fn composite_preview_with_stats(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        stats: &mut super::compositing::CompositeStats,
    ) -> Option<DynamicImage> {
        self.composite_scope_with(&self.root, resolve, stats)
    }

    /// Corps de [`Self::composite_scope`] à résolveur injecté.
    fn composite_scope_with(
        &self,
        nodes: &[LayerNode],
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        stats: &mut super::compositing::CompositeStats,
    ) -> Option<DynamicImage> {
        let (half_w, half_h) = scope_half_extents(nodes, self.width, self.height, resolve);
        // Clamp pour éviter OOM (16384 ≈ 1 Go RGBA)
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        let t_alloc = std::time::Instant::now();
        let mut acc = ImageBuffer::from_pixel(w.max(1), h.max(1), Rgba([0, 0, 0, 0]));
        stats.acc_alloc_us += t_alloc.elapsed().as_micros();
        stats.scope_px = u64::from(acc.width()) * u64::from(acc.height());
        // Origine monde (0,0) = coin du buffer moins demi-tailles
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        if !super::compositing::fold_scope_stats(
            nodes, &mut acc, origin_x, origin_y, resolve, stats,
        ) {
            return None; // aucun calque visible/contribuant
        }
        Some(DynamicImage::ImageRgba8(acc))
    }

    /// Composite CROPÉ aux dimensions du document — utilisé pour l'export.
    pub fn composite(&self) -> Option<DynamicImage> {
        let img = self.composite_preview()?;
        let (w, h) = img.dimensions();
        if w <= self.width && h <= self.height {
            return Some(img);
        }
        let x = w.saturating_sub(self.width) / 2;
        let y = h.saturating_sub(self.height) / 2;
        Some(img.crop_imm(x, y, self.width.max(1), self.height.max(1)))
    }

    /// Statistiques du renderer — instrumentation des tests (cache chaud).
    #[cfg(test)]
    pub fn renderer_stats(&self) -> (u64, u64) {
        let r = self.cache.borrow();
        (r.hits(), r.misses())
    }

    /// Compteurs d'apparences (hits, misses, rebuilds preview/thumb) —
    /// observabilité du partage inter-passes, sans effet sur le rendu.
    pub fn appearance_stats(&self) -> crate::renderer::AppearanceStats {
        self.cache.borrow().stats()
    }
}

fn find_in(nodes: &[LayerNode], id: Uuid) -> Option<&LayerNode> {
    for n in nodes {
        if n.id() == id {
            return Some(n);
        }
        if let LayerNode::Group(g) = n
            && let Some(found) = find_in(&g.children, id)
        {
            return Some(found);
        }
    }
    None
}

fn find_in_mut(nodes: &mut [LayerNode], id: Uuid) -> Option<&mut LayerNode> {
    for n in nodes {
        if n.id() == id {
            return Some(n);
        }
        if let LayerNode::Group(g) = n
            && let Some(found) = find_in_mut(&mut g.children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Vrai si au moins un nœud contribuerait au composite (mêmes gardes
/// que [`fold_scope`](super::compositing::fold_scope), sans rendre).
/// Un ajustement seul ne compte pas : sur accumulateur vide il ne
/// produit rien, comme le composite qui retourne alors `None`.
fn contributes(nodes: &[LayerNode], resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>) -> bool {
    for node in nodes {
        match node {
            LayerNode::Pixel(l) => {
                if l.visible && l.opacity > 0.01 && resolve(l.id).is_some() {
                    return true;
                }
            }
            LayerNode::Group(g) => {
                if g.visible && g.opacity > 0.01 && contributes(&g.children, resolve) {
                    return true;
                }
            }
            LayerNode::Adjustment(_) => {}
        }
    }
    false
}

/// Parent (`None` = racine) et index d'un nœud, pour détecter les no-ops.
fn owner_position(
    nodes: &[LayerNode],
    id: Uuid,
    parent: Option<Uuid>,
) -> Option<(Option<Uuid>, usize)> {
    for (index, node) in nodes.iter().enumerate() {
        if node.id() == id {
            return Some((parent, index));
        }
        if let LayerNode::Group(groupe) = node
            && let Some(found) = owner_position(&groupe.children, id, Some(groupe.id))
        {
            return Some(found);
        }
    }
    None
}

/// Trouve la liste possédant `id` (racine ou enfants d'un groupe) + index.
fn find_owner_list(nodes: &mut Vec<LayerNode>, id: Uuid) -> Option<(&mut Vec<LayerNode>, usize)> {
    if let Some(idx) = nodes.iter().position(|n| n.id() == id) {
        return Some((nodes, idx));
    }
    for n in nodes.iter_mut() {
        if let LayerNode::Group(g) = n
            && let Some(found) = find_owner_list(&mut g.children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Masque récursivement un sous-arbre (drag d'un groupe = tout le groupe).
fn hide_subtree(node: Option<&mut LayerNode>) {
    let Some(node) = node else { return };
    node.set_visible(false);
    if let LayerNode::Group(g) = node {
        for child in &mut g.children {
            hide_subtree(Some(child));
        }
    }
}

fn collect_pixels<'a>(nodes: &'a [LayerNode], out: &mut Vec<&'a PixelLayer>) {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                if l.visible && l.opacity > 0.01 {
                    out.push(l);
                }
            }
            LayerNode::Group(g) => {
                if !g.visible || g.opacity <= 0.01 {
                    continue;
                }
                collect_pixels(&g.children, out)
            }
            LayerNode::Adjustment(_) => {}
        }
    }
}

/// Ids de tous les calques pixels, sans filtre de visibilité (le
/// snapshot a besoin des miniatures même des calques masqués).
fn collect_all_pixel_ids(nodes: &[LayerNode], out: &mut Vec<Uuid>) {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                out.push(l.id);
            }
            LayerNode::Group(g) => collect_all_pixel_ids(&g.children, out),
            LayerNode::Adjustment(_) => {}
        }
    }
}

fn collect_masks<'a>(
    nodes: &'a [LayerNode],
    out: &mut Vec<(Uuid, &'a crate::document::LayerMask)>,
) {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                for m in &l.masks {
                    out.push((l.id, m));
                }
                // Masques des sous-calques de filtres (porteur = id du filtre)
                for f in &l.filter_layers {
                    for m in &f.masks {
                        out.push((f.id, m));
                    }
                }
            }
            LayerNode::Group(g) => {
                for m in &g.masks {
                    out.push((g.id, m));
                }
                collect_masks(&g.children, out)
            }
            LayerNode::Adjustment(_) => {}
        }
    }
}

/// Sous-calque de filtre par son id (pixels racine ou groupes imbriqués).
fn find_filter_in(nodes: &[LayerNode], filter_id: Uuid) -> Option<&FilterLayer> {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                if let Some(f) = l.filter_layers.iter().find(|f| f.id == filter_id) {
                    return Some(f);
                }
            }
            LayerNode::Group(g) => {
                if let Some(found) = find_filter_in(&g.children, filter_id) {
                    return Some(found);
                }
            }
            LayerNode::Adjustment(_) => {}
        }
    }
    None
}

fn find_filter_in_mut(nodes: &mut [LayerNode], filter_id: Uuid) -> Option<&mut FilterLayer> {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                if let Some(f) = l.filter_layers.iter_mut().find(|f| f.id == filter_id) {
                    return Some(f);
                }
            }
            LayerNode::Group(g) => {
                if let Some(found) = find_filter_in_mut(&mut g.children, filter_id) {
                    return Some(found);
                }
            }
            LayerNode::Adjustment(_) => {}
        }
    }
    None
}

/// Calque pixels porteur d'un sous-calque de filtre.
fn find_filter_parent_in(nodes: &[LayerNode], filter_id: Uuid) -> Option<Uuid> {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                if l.filter_layers.iter().any(|f| f.id == filter_id) {
                    return Some(l.id);
                }
            }
            LayerNode::Group(g) => {
                if let Some(found) = find_filter_parent_in(&g.children, filter_id) {
                    return Some(found);
                }
            }
            LayerNode::Adjustment(_) => {}
        }
    }
    None
}

/// Porteur d'un masque par son id — un nœud (pixel/groupe) dont la liste
/// `masks` le contient, OU le sous-calque de filtre qui le porte.
fn mask_owner_in(nodes: &[LayerNode], mask_id: Uuid) -> Option<Uuid> {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                if l.masks.iter().any(|m| m.id == mask_id) {
                    return Some(l.id);
                }
                for f in &l.filter_layers {
                    if f.masks.iter().any(|m| m.id == mask_id) {
                        return Some(f.id);
                    }
                }
            }
            LayerNode::Adjustment(_) => {}
            LayerNode::Group(g) => {
                if g.masks.iter().any(|m| m.id == mask_id) {
                    return Some(g.id);
                }
                if let Some(found) = mask_owner_in(&g.children, mask_id) {
                    return Some(found);
                }
            }
        }
    }
    None
}
