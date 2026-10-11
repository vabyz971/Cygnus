use super::compositing::{apply_coverage, needs_fallback_in, scope_half_extents};
use super::model::{
    Appearance, BlendMode, FilterLayer, FilterNode, GroupLayer, LayerMask, LayerNode, PixelLayer,
    RgbaBuf, Transform2D,
};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use std::cell::RefCell;
use std::sync::Arc;
use tiles::TileRegion;
use uuid::Uuid;

pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Root stack — index 0 = BOTTOM of the stack (drawn first)
    pub root: Vec<LayerNode>,
    /// Per-layer appearance cache ([`crate::renderer::Renderer`]):
    /// validity by filter signature + source identity, fed by the
    /// GPU compute / rayon CPU chain. Interior mutability since the
    /// cache is a performance detail invisible from the API (&self).
    cache: RefCell<crate::renderer::Renderer>,
}

/// Result of an experimental regional composite (tile vertical
/// slice): region pixels + origin for stitching/comparing in
/// full-frame composite space.
///
/// `Clone`: `TileCache` (Phase 6D) re-serves the tile by cloning
/// (pixel copy — correctness-first foundation, later `Arc` sharing
/// if measurements require it).
#[derive(Debug, Clone)]
pub struct RegionalComposite {
    /// RGBA8 pixels of the clipped region (dimensions = useful region).
    pub image: DynamicImage,
    /// Buffer column (full-frame space) of the top-left corner.
    pub origin_x: u32,
    /// Buffer row (full-frame space) of the top-left corner.
    pub origin_y: u32,
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

    /// Blank document with a first opaque WHITE layer at the
    /// document dimensions (« Créer un document » without an image,
    /// Affinity-style: white background, no transparent checkerboard).
    pub fn with_blank_layer(width: u32, height: u32) -> Self {
        let mut doc = Self::new(width.max(1), height.max(1));
        let white =
            image::RgbaImage::from_pixel(doc.width, doc.height, image::Rgba([255, 255, 255, 255]));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new(
            "Calque 1",
            Arc::new(DynamicImage::ImageRgba8(white)),
        )));
        doc
    }

    /// Rebuilds the document from a restored state (undo/redo, project). The
    /// appearance cache is emptied: restored entries revalidate by
    /// signature on first request.
    pub fn restore(&mut self, width: u32, height: u32, root: Vec<LayerNode>) {
        self.width = width;
        self.height = height;
        self.root = root;
        self.cache.borrow_mut().invalidate_all();
    }

    // -- Lookup ----------------------------------------------------------

    pub fn find(&self, id: Uuid) -> Option<&LayerNode> {
        find_in(&self.root, id)
    }

    pub fn find_mut(&mut self, id: Uuid) -> Option<&mut LayerNode> {
        find_in_mut(&mut self.root, id)
    }

    /// Typed access to the pixel layer.
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

    /// Flat list of pixel layers, draw order (bottom → top), DFS.
    #[must_use]
    pub fn iter_pixels(&self) -> Vec<&PixelLayer> {
        let mut out = Vec::with_capacity(self.root.len());
        collect_pixels(&self.root, &mut out);
        out
    }

    /// Ids of ALL pixel layers (visible or not, groups included), DFS
    /// order. Used for inter-pass sharing: snapshots need thumbnails
    /// even of hidden layers (unlike [`Self::iter_pixels`], which
    /// only keeps contributors).
    #[must_use]
    pub fn all_pixel_ids(&self) -> Vec<Uuid> {
        let mut out = Vec::new();
        collect_all_pixel_ids(&self.root, &mut out);
        out
    }

    pub fn pixel_count(&self) -> usize {
        self.iter_pixels().len()
    }

    /// Flat list of every mask in the tree, each with its carrier layer
    /// id (pixel layers AND groups). Order: DFS, bottom → top.
    #[must_use]
    pub fn iter_masks(&self) -> Vec<(Uuid, &crate::document::LayerMask)> {
        let mut out = Vec::new();
        collect_masks(&self.root, &mut out);
        out
    }

    /// Is the fast "1 texture per layer" render possible, or must the CPU
    /// composite be used? True as soon as a group has a non-Normal
    /// blend mode or an adjustment layer acts on the stack.
    pub fn needs_fallback(&self) -> bool {
        needs_fallback_in(&self.root)
    }

    // -- Structural mutations ---------------------------------------------

    /// Adds a node at the TOP of the root stack.
    pub fn push_layer(&mut self, node: LayerNode) {
        self.root.push(node);
    }

    /// Inserts `node` just above `anchor` (same parent).
    pub fn insert_above(&mut self, anchor: Uuid, node: LayerNode) -> bool {
        match find_owner_list(&mut self.root, anchor) {
            Some((list, idx)) => {
                list.insert(idx + 1, node);
                true
            }
            None => false,
        }
    }

    /// Detaches subtree `id` from its parent.
    pub fn remove(&mut self, id: Uuid) -> Option<LayerNode> {
        let (list, idx) = find_owner_list(&mut self.root, id)?;
        Some(list.remove(idx))
    }

    /// Duplicates the subtree (fresh ids everywhere) and inserts it above.
    pub fn duplicate(&mut self, id: Uuid) -> Option<Uuid> {
        let mut copy = self.find(id)?.clone();
        copy.regenerate_ids();
        let new_id = copy.id();
        if !self.insert_above(id, copy) {
            return None;
        }
        Some(new_id)
    }

    /// Moves up one slot among siblings (toward the top of the stack).
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

    /// Moves down one slot among siblings.
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

    /// Toggles a group's collapsed/expanded state (Layers panel display
    /// only: no pixels touched). False if `id` is
    /// not a group.
    pub fn set_collapsed(&mut self, id: Uuid, collapsed: bool) -> bool {
        match self.find_mut(id) {
            Some(LayerNode::Group(groupe)) => {
                groupe.collapsed = collapsed;
                true
            }
            _ => false,
        }
    }

    /// Reorders via drag & drop: moves `dragged` before or after `target`.
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
            // target gone? restore at the end
            self.push_layer(node);
            return false;
        };
        let at = if before { idx } else { idx + 1 };
        let at = at.min(list.len());
        list.insert(at, node);
        true
    }

    /// Is the before/after move structurally valid?
    ///
    /// An adjacent no-op move stays a valid target: it will simply be
    /// ignored at commit, with no history entry.
    #[must_use]
    pub fn can_reorder_before(&self, dragged: Uuid, target: Uuid) -> bool {
        if dragged == target {
            return false;
        }
        if self.find(dragged).is_none() || self.find(target).is_none() {
            return false;
        }
        // Prevents moving a node into its own subtree
        if let Some(dragged_node) = self.find(dragged)
            && Self::contains_id(dragged_node.children(), target)
        {
            return false;
        }
        true
    }

    /// Moves `dragged` to the head of `group`'s children.
    pub fn move_into(&mut self, dragged: Uuid, group: Uuid) -> bool {
        if !self.can_move_into(dragged, group) || self.is_noop_move_into(dragged, group) {
            return false;
        }
        let Some(node) = self.remove(dragged) else {
            return false;
        };
        let Some(LayerNode::Group(groupe)) = self.find_mut(group) else {
            // group gone? restore at the end
            self.push_layer(node);
            return false;
        };
        groupe.children.insert(0, node);
        true
    }

    /// Is head-of-group insertion structurally valid?
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
        if let Some(dragged_node) = self.find(dragged)
            && Self::contains_id(dragged_node.children(), group)
        {
            return false;
        }
        true
    }

    /// Nests `node` at the head of `parent`'s children (group, or pixel
    /// layer as an Affinity-style clipped child). Returns false if the
    /// parent accepts no children (adjustment) or is unknown. The parent
    /// is touched (appearance invalidation) if it is pixels.
    pub fn add_child(&mut self, parent: Uuid, node: LayerNode) -> bool {
        let Some(target) = self.find_mut(parent) else {
            return false;
        };
        let Some(children) = target.children_mut() else {
            return false;
        };
        children.insert(0, node);
        if let LayerNode::Pixel(l) = target {
            l.touch();
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
            if Self::contains_id(n.children(), id) {
                return true;
            }
        }
        false
    }

    /// Groups the given nodes (same siblings) into a new group inserted
    /// in place of the lowest one. The stack's relative order is
    /// preserved. Returns the created group's id.
    pub fn group(&mut self, ids: &[Uuid]) -> Option<Uuid> {
        let first = *ids.first()?;
        let (list, _) = find_owner_list(&mut self.root, first)?;
        // Positions computed once, sorted (stack order)
        let mut idxs: Vec<usize> = Vec::with_capacity(ids.len());
        for id in ids {
            idxs.push(list.iter().position(|n| n.id() == *id)?);
        }
        idxs.sort_unstable();
        // Extraction highest to lowest, then back in order
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

    /// Dissolves a group: its children move up in its place in the parent.
    /// Returns the freed children's ids.
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

    // -- Destructive (pixel) edits ---------------------------------------

    /// Flips the layer horizontally/vertically (destructive).
    ///
    /// # Errors
    /// Returns an error if the layer does not exist.
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
        // Sub-layer masks live in the same source space
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

    /// Rotates the layer 90° (destructive, clockwise if `clockwise`).
    ///
    /// Dimensions are swapped: the offset compensates to keep the CENTER
    /// in place in world space. Masks (layer + sub-layers) rotated in
    /// the same source space, like [`Self::flip`].
    ///
    /// # Errors
    /// Returns an error if the layer does not exist.
    pub fn rotate(&mut self, id: Uuid, clockwise: bool) -> Result<(), String> {
        let layer = self.pixel_layer_mut(id).ok_or("calque introuvable")?;
        let (old_w, old_h) = layer.dimensions();
        let rotated = if clockwise {
            layer.source_image.rotate90()
        } else {
            layer.source_image.rotate270()
        };
        for mask in &mut layer.masks {
            let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
            let rotated_mask = if clockwise {
                dyn_mask.rotate90()
            } else {
                dyn_mask.rotate270()
            }
            .to_rgba8();
            mask.image = Arc::new(rotated_mask);
            mask.touch();
        }
        // Sub-layer masks live in the same source space
        for f in &mut layer.filter_layers {
            for mask in &mut f.masks {
                let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
                let rotated_mask = if clockwise {
                    dyn_mask.rotate90()
                } else {
                    dyn_mask.rotate270()
                }
                .to_rgba8();
                mask.image = Arc::new(rotated_mask);
                mask.touch();
            }
        }
        layer.set_source_image(rotated);
        // Center compensation: (w,h) → (h,w).
        let (new_w, new_h) = layer.dimensions();
        layer.transform.offset_x += (old_w as f32 - new_w as f32) / 2.0;
        layer.transform.offset_y += (old_h as f32 - new_h as f32) / 2.0;
        Ok(())
    }

    /// Paints into a mask (white = reveals if `whiten`, black = hides
    /// otherwise), in layer source space (same space as the worker
    /// points, like pixel paint).
    ///
    /// The brush reuses `paint_stroke_rgba` in `Paint` mode (never
    /// `Erase`: eraser on mask = black, not destination-out). Coverage
    /// = R channel (see `mask_coverage`).
    ///
    /// # Errors
    /// Returns an error if the carrier or the mask is not found.
    pub fn paint_mask(
        &mut self,
        owner_id: Uuid,
        mask_id: Uuid,
        points: &[(f32, f32)],
        radius: f32,
        opacity: f32,
        whiten: bool,
    ) -> Result<(), String> {
        if points.is_empty() || !radius.is_finite() || radius <= 0.0 {
            return Err("trait vide".into());
        }
        let Some(masks) = self.masks_of_mut(owner_id) else {
            return Err("porteur introuvable".into());
        };
        let Some(mask) = masks.iter_mut().find(|m| m.id == mask_id) else {
            return Err("masque introuvable".into());
        };
        let (w, h) = (mask.image.width(), mask.image.height());
        let mut buffer = mask.image.as_raw().to_vec();
        let v = if whiten { 255 } else { 0 };
        let brush = crate::paint::BrushParams {
            radius,
            color: [v, v, v],
            opacity: opacity.clamp(0.0, 1.0),
            mode: crate::paint::StrokeMode::Paint,
        };
        crate::paint::paint_stroke_rgba(&mut buffer, w, h, points, &brush);
        let Some(image) = image::RgbaImage::from_raw(w, h, buffer) else {
            return Err("rastérisation impossible".into());
        };
        mask.image = Arc::new(image);
        mask.touch();
        if self.find(owner_id).is_some() {
            self.touch_pixel(owner_id);
        } else if let Some(parent) = self.find_filter_parent(owner_id) {
            self.touch_pixel(parent);
        }
        Ok(())
    }

    /// Crops the layer to the rect (LAYER coordinates, pixels). Destructive:
    /// content stays put in world space (the transform compensates the
    /// crop origin). Descriptive error if the rect is invalid.
    ///
    /// # Errors
    /// Returns an error if the layer is not found or the rectangle exceeds the edges.
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
        // Compensate the origin: the original (x,y) pixel stays at its world spot
        layer.transform.offset_x += x as f32;
        layer.transform.offset_y += y as f32;
        // Crop the linked masks at the same coordinates
        for mask in &mut layer.masks {
            let dyn_mask = DynamicImage::ImageRgba8((*mask.image).clone());
            let cropped_mask = dyn_mask.crop_imm(x as u32, y as u32, w, h).to_rgba8();
            mask.image = Arc::new(cropped_mask);
            mask.touch();
        }
        // Same for sub-layer masks (same source space)
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

    /// Replaces a pixel layer's source image (paint…).
    pub fn set_source_image(&mut self, id: Uuid, image: DynamicImage) -> bool {
        match self.pixel_layer_mut(id) {
            Some(layer) => {
                layer.set_source_image(image);
                true
            }
            None => false,
        }
    }

    // -- Dynamic filters ----------------------------------------------------

    /// Appends a filter sub-layer at the end of a pixel layer or group
    /// chain (group nested filter, applied after its children). For an
    /// adjustment, the sub-layer is converted to a plain filter
    /// (adjustments carry opacity/blend at the layer level). Returns
    /// the inserted filter's id.
    pub fn add_filter(&mut self, layer_id: Uuid, filter: FilterLayer) -> Option<Uuid> {
        let fid = filter.id;
        match self.find_mut(layer_id)? {
            LayerNode::Pixel(l) => l.filter_layers.push(filter),
            LayerNode::Group(g) => g.filter_layers.push(filter),
            LayerNode::Adjustment(a) => a.filters.push(FilterNode {
                id: filter.id,
                type_id: filter.type_id,
                params: filter.params,
                enabled: filter.enabled,
            }),
        }
        self.touch_pixel(layer_id);
        Some(fid)
    }

    /// Removes a filter sub-layer (pixels, group) or an adjustment
    /// filter.
    pub fn remove_filter(&mut self, layer_id: Uuid, filter_id: Uuid) -> Option<FilterLayer> {
        let removed = match self.find_mut(layer_id)? {
            LayerNode::Pixel(l) => {
                let idx = l.filter_layers.iter().position(|f| f.id == filter_id)?;
                l.filter_layers.remove(idx)
            }
            LayerNode::Group(g) => {
                let idx = g.filter_layers.iter().position(|f| f.id == filter_id)?;
                g.filter_layers.remove(idx)
            }
            LayerNode::Adjustment(a) => {
                let idx = a.filters.iter().position(|f| f.id == filter_id)?;
                let f = a.filters.remove(idx);
                FilterLayer::from_node(f)
            }
        };
        self.touch_pixel(layer_id);
        Some(removed)
    }

    /// Edits a filter parameter (continuous gesture: app-side coalescing).
    /// Covers pixel and group sub-layers, and adjustment plain
    /// filters.
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
            Some(LayerNode::Group(g)) => g
                .filter_layers
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.params),
            Some(LayerNode::Adjustment(a)) => a
                .filters
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.params),
            None => None,
        };
        let Some(params) = params else {
            return false;
        };
        params.insert(key, value);
        self.touch_pixel(layer_id);
        true
    }

    /// Enables/disables a filter without losing its settings (pixels,
    /// nested groups, adjustments).
    pub fn set_filter_enabled(&mut self, layer_id: Uuid, filter_id: Uuid, enabled: bool) -> bool {
        let target = match self.find_mut(layer_id) {
            Some(LayerNode::Pixel(l)) => l
                .filter_layers
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.enabled),
            Some(LayerNode::Group(g)) => g
                .filter_layers
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.enabled),
            Some(LayerNode::Adjustment(a)) => a
                .filters
                .iter_mut()
                .find(|f| f.id == filter_id)
                .map(|f| &mut f.enabled),
            None => None,
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

    /// Enables/disables a mask without losing its pixels.
    ///
    /// The carrier may be a node or a filter sub-layer (like
    /// [`Self::masks_of_mut`]): the touch follows the root pixel
    /// layer to invalidate appearance.
    pub fn set_mask_enabled(&mut self, owner_id: Uuid, mask_id: Uuid, enabled: bool) -> bool {
        let Some(masks) = self.masks_of_mut(owner_id) else {
            return false;
        };
        let Some(mask) = masks.iter_mut().find(|m| m.id == mask_id) else {
            return false;
        };
        if mask.enabled == enabled {
            return true;
        }
        mask.enabled = enabled;
        mask.touch();
        // Invalidate the root pixel layer's appearance.
        if self.find(owner_id).is_some() {
            self.touch_pixel(owner_id);
        } else if let Some(parent) = self.find_filter_parent(owner_id) {
            self.touch_pixel(parent);
        }
        true
    }

    /// Moves a filter sub-layer within its parent chain (pixels or
    /// group — order = application order, Affinity-style).
    pub fn move_filter(&mut self, layer_id: Uuid, filter_id: Uuid, up: bool) -> bool {
        let Some(node) = self.find_mut(layer_id) else {
            return false;
        };
        let Some(filters) = node.filter_layers_mut() else {
            return false;
        };
        let Some(idx) = filters.iter().position(|f| f.id == filter_id) else {
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
        if other >= filters.len() {
            return false;
        }
        filters.swap(idx, other);
        if let LayerNode::Pixel(l) = node {
            l.touch();
        }
        true
    }

    /// Duplicates a filter sub-layer (fresh id) just above (pixels
    /// or group).
    pub fn duplicate_filter(&mut self, layer_id: Uuid, filter_id: Uuid) -> Option<Uuid> {
        let node = self.find_mut(layer_id)?;
        let filters = node.filter_layers_mut()?;
        let idx = filters.iter().position(|f| f.id == filter_id)?;
        let mut copy = filters[idx].clone();
        copy.id = Uuid::new_v4();
        // Masks are addressed by id (thumbnail cache): fresh ids.
        for m in &mut copy.masks {
            m.id = Uuid::new_v4();
        }
        let new_id = copy.id;
        filters.insert(idx + 1, copy);
        if let LayerNode::Pixel(l) = node {
            l.touch();
        }
        Some(new_id)
    }

    fn touch_pixel(&mut self, layer_id: Uuid) {
        if let Some(LayerNode::Pixel(l)) = self.find_mut(layer_id) {
            l.touch();
        }
    }

    // -- Sub-layer lookup -------------------------------------------

    /// Filter sub-layer by its id (any pixel layer, root or nested
    /// group).
    pub fn find_filter_layer(&self, filter_id: Uuid) -> Option<&FilterLayer> {
        find_filter_in(&self.root, filter_id)
    }

    pub fn find_filter_layer_mut(&mut self, filter_id: Uuid) -> Option<&mut FilterLayer> {
        find_filter_in_mut(&mut self.root, filter_id)
    }

    /// Pixel layer carrying a filter sub-layer.
    pub fn find_filter_parent(&self, filter_id: Uuid) -> Option<Uuid> {
        find_filter_parent_in(&self.root, filter_id)
    }

    // -- Polymorphic attributes (node OR filter sub-layer) ---------------

    /// Node (pixels) or filter sub-layer transform.
    pub fn transform_of(&self, id: Uuid) -> Option<Transform2D> {
        match self.find(id) {
            Some(LayerNode::Pixel(l)) => Some(l.transform),
            _ => self.find_filter_layer(id).map(|f| f.transform),
        }
    }

    /// Replaces the transform (touches the carrier layer for invalidation).
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

    /// 0..=100 opacity of the node or filter sub-layer.
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

    /// Blend mode of the node or filter sub-layer.
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

    /// Renames a node, filter sub-layer or mask.
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

    /// Carrier of a mask (node or filter sub-layer) — `None` otherwise.
    pub fn mask_owner_of(&self, mask_id: Uuid) -> Option<Uuid> {
        mask_owner_in(&self.root, mask_id)
    }

    /// Display name of a mask by its id.
    pub fn mask_name(&self, mask_id: Uuid) -> Option<String> {
        let owner = self.mask_owner_of(mask_id)?;
        self.masks_of(owner)?
            .iter()
            .find(|m| m.id == mask_id)
            .map(|m| m.name.clone())
    }

    /// Moves a mask within its carrier's list. `up = true` → toward the
    /// displayed stack TOP (decreasing index, mask list = display
    /// order). Returns `false` at the limits or when not found.
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

    // -- Masks (nodes AND filter sub-layers) ----------------------------

    /// Mask by (carrier, id) — the carrier is a node or a sub-layer.
    pub fn mask_of(&self, owner_id: Uuid, mask_id: Uuid) -> Option<&LayerMask> {
        self.masks_of(owner_id)?.iter().find(|m| m.id == mask_id)
    }

    pub fn mask_of_mut(&mut self, owner_id: Uuid, mask_id: Uuid) -> Option<&mut LayerMask> {
        self.masks_of_mut(owner_id)?
            .iter_mut()
            .find(|m| m.id == mask_id)
    }

    /// All masks of a carrier (node or filter sub-layer).
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

    // -- History commands ------------------------------------------------

    /// Applies `command.new` to the document and returns the INVERSE
    /// (old/new swapped) ready to push for redo.
    ///
    /// Systematic routing through existing setters: model invariants are
    /// preserved (opacity clamp, appearance-version bump for targeted
    /// cache invalidation, scale clamp).
    ///
    /// If the target node is gone, the command is returned as is: pushing
    /// it stays safe (reapplication = no-op).
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

    // -- Appearance ------------------------------------------------------------

    /// Layer derived appearance (source × active filters), served by the
    /// [`crate::renderer::Renderer`]: HIT = zero recompute, MISS = chain
    /// execution (compute shaders when a GPU is available). Returns cheap
    /// clones (Arc/RgbaBuf).
    pub fn appearance(&self, id: Uuid) -> Option<Appearance> {
        let layer = self.pixel_layer(id)?;
        Some(self.cache.borrow_mut().appearance(layer))
    }

    /// [`Self::appearance`] variant with NO execution: returns the
    /// appearance if the cache still holds it valid, `None` otherwise.
    /// Used by UI sync on every message — when warm, it touches
    /// neither the render pool nor the filter chain.
    pub fn appearance_hit(&self, id: Uuid) -> Option<Appearance> {
        let layer = self.pixel_layer(id)?;
        self.cache.borrow_mut().appearance_hit(layer)
    }

    /// Image alone (compositing path — avoids regenerating preview/thumb).
    pub fn appearance_image(&self, id: Uuid) -> Option<Arc<DynamicImage>> {
        self.appearance(id).map(|a| a.image)
    }

    /// Image alone WITHOUT thumbnail or preview (Phase 6G.3): composes the
    /// already-cacheable pieces (`unmasked_image` + `mask_coverage`, same
    /// guards as the full path) and bakes. Pixels bit-identical to
    /// [`Self::appearance`]; shared `hits`/`misses` counters; no
    /// `thumb`/`preview_rebuild` counted here (thumbnails go to a
    /// background task — see the app worker).
    pub fn appearance_image_only(&self, id: Uuid) -> Option<Arc<DynamicImage>> {
        let layer = self.pixel_layer(id)?;
        let mut cache = self.cache.borrow_mut();
        let unmasked = cache.unmasked_image(layer);
        match cache.mask_coverage(layer) {
            None => Some(unmasked),
            Some(cover) => Some(apply_coverage(unmasked, &cover)),
        }
    }

    /// Image + thumbnail for the response frame (Phase 6G.2 §2): never
    /// derives the `preview` buffer (unread in production). Pixels
    /// bit-identical to [`Self::appearance`]; shared `hits` / `misses` /
    /// `thumb_rebuilds` counters (`preview_rebuilds` untouched: no
    /// preview rebuilt here).
    pub fn appearance_frame(&self, id: Uuid) -> Option<(Arc<DynamicImage>, RgbaBuf)> {
        let layer = self.pixel_layer(id)?;
        self.cache.borrow_mut().appearance_frame(layer)
    }

    /// Exports a pixel layer's warm appearance entry (transfer to the live
    /// document after `spawn_blocking` computation — see
    /// [`crate::renderer::WarmedAppearance`]).
    pub fn export_warmed(&self, id: Uuid) -> Option<crate::renderer::WarmedAppearance> {
        self.cache.borrow().export_warmed(id)
    }

    /// Inserts an entry pre-computed off the UI thread: the next
    /// `appearance_hit` HITs without running the chain on the UI.
    pub fn insert_warmed(&mut self, id: Uuid, warmed: crate::renderer::WarmedAppearance) {
        self.cache.borrow_mut().insert_warmed(id, warmed);
    }

    /// Thumbnail for the Layers panel (derived appearance).
    pub fn thumb(&self, id: Uuid) -> Option<RgbaBuf> {
        self.appearance(id).map(|a| a.thumb)
    }

    /// Warms THIS document's appearance cache from the entries currently
    /// warm in another document (typically the LIVE document, for a clone
    /// sent to a background task). Avoids re-running the render chains
    /// (preview/thumb filters) on every background composite: unchanged
    /// layers HIT instead of MISS.
    pub fn warm_cache_from(&mut self, other: &Document) {
        if std::ptr::eq(self, other) {
            return;
        }
        let mut dst = self.cache.borrow_mut();
        let src = other.cache.borrow();
        dst.import_from(&src);
    }

    // -- History -------------------------------------------------------------

    /// Full snapshot (Arc-shared pixels — nearly free).
    pub fn snapshot(&self) -> crate::history::Snapshot {
        crate::history::Snapshot {
            doc_size: (self.width, self.height),
            root: self.root.clone(),
        }
    }

    /// Restores a snapshot.
    pub fn restore_snapshot(&mut self, snap: crate::history::Snapshot) {
        self.restore(snap.doc_size.0, snap.doc_size.1, snap.root);
    }

    // -- Compositing ---------------------------------------------------------------

    /// Composite for the infinite canvas: no cropping to the document. The
    /// document stays centered (like Affinity/Photoshop) and outside layers
    /// stay visible. Returns None if nothing is visible.
    pub fn composite_preview(&self) -> Option<DynamicImage> {
        self.composite_scope(&self.root)
    }

    /// Infinite-plane composite WITHOUT the given subtree — used for the
    /// pre-computed background during a drag. Reuses THIS document's
    /// APPEARANCE CACHE: all remaining layers HIT, cost is limited to
    /// the blend itself (unlike cloning into a fresh document whose
    /// cache is cold).
    pub fn composite_preview_without(&self, exclude_id: Uuid) -> Option<DynamicImage> {
        // Cheap structural clone (shared Arcs), subtree masked out, then
        // composite via THE SAME cache as the live document.
        let mut hidden = self.root.clone();
        hide_subtree(find_in_mut(&mut hidden, exclude_id));
        if hidden.is_empty() {
            return None;
        }
        self.composite_scope(&hidden)
    }

    /// Samples the composited color at a point in local document
    /// coordinates (0,0 = DOCUMENT top-left corner). Returns [R,G,B,A]
    /// or `None` if outside the composite plane or the document is empty.
    /// Designed to be called ONLY in `spawn_blocking` — performs a full
    /// composite if needed.
    pub fn sample_color(&self, dx: f32, dy: f32) -> Option<[u8; 4]> {
        // Same geometry and origin as `composite_scope`: re-deriving them
        // from the buffer's ROUNDED dimensions would break the mapping as
        // soon as extents are fractional.
        let resolver = |id: Uuid| self.appearance_image(id);
        let (half_w, half_h) = scope_half_extents(&self.root, self.width, self.height, &resolver);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        // The composite places pixels at the document's "floor" index (blend_into
        // rounds and stamps at exactly `offset`): the pixel occupying doc
        // cell [p, p+1) lives at index p — floor(), not round().
        let px = (origin_x + dx).floor() as i64;
        let py = (origin_y + dy).floor() as i64;
        if px < 0 || py < 0 || px >= i64::from(w) || py >= i64::from(h) {
            return None;
        }
        let img = self.composite_preview()?;
        let p = img.get_pixel(px as u32, py as u32);
        Some([p[0], p[1], p[2], p[3]])
    }

    /// Samples a square `side`×`side` PATCH (RGBA8, row-major) centered
    /// on document point `(dx, dy)` — outside-composite-plane pixels
    /// transparent. Feeds the eyedropper MAGNIFIER (Photoshop-style
    /// zoom). Same full composite as [`Self::sample_color`] → call ONLY
    /// in `spawn_blocking`.
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

    /// Infinite-plane composite geometry: full-resolution dimensions
    /// `(w, h)` and DOCUMENT top-left corner `(ox, oy)` within those
    /// pixels. `None` if nothing is visible. Half-extents are symmetric
    /// by construction (`scope_half_extents`): the document is therefore
    /// always centered in the buffer.
    /// Shared with the UI (thumbnail preview) to convert
    /// screen→document pixels without drift.
    pub fn preview_geometry(&self) -> Option<(u32, u32, f32, f32)> {
        self.preview_geometry_of(&self.root)
    }

    /// [`Self::preview_geometry`] variant with an external resolver:
    /// shares appearances already resolved during the response
    /// (zero new resolution). Behavior identical for an equal
    /// resolver.
    pub fn preview_geometry_with(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<(u32, u32, f32, f32)> {
        self.preview_geometry_of_with(&self.root, resolve)
    }

    /// [`Self::preview_geometry`] variant over a given scope.
    fn preview_geometry_of(&self, nodes: &[LayerNode]) -> Option<(u32, u32, f32, f32)> {
        let resolver = |id: Uuid| self.appearance_image(id);
        self.preview_geometry_of_with(nodes, &resolver)
    }

    /// [`Self::preview_geometry_of`] body with injected resolver.
    fn preview_geometry_of_with(
        &self,
        nodes: &[LayerNode],
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<(u32, u32, f32, f32)> {
        let (half_w, half_h) = scope_half_extents(nodes, self.width, self.height, resolve);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        // Zero cost if nothing contributes (avoids a ghost composite).
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

    /// Infinite-plane composite with external resolver: shares
    /// appearances already resolved during the response (zero new
    /// resolution). Identical behavior for an equal resolver.
    pub fn composite_preview_with(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
    ) -> Option<DynamicImage> {
        let mut stats = super::compositing::CompositeStats::default();
        self.composite_scope_with(&self.root, resolve, &mut stats)
    }

    /// Instrumented [`Self::composite_preview_with`] variant: fills
    /// `stats` (scope, accumulator alloc, blends, traversed pixels).
    /// Identical render behavior.
    pub fn composite_preview_with_stats(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        stats: &mut super::compositing::CompositeStats,
    ) -> Option<DynamicImage> {
        self.composite_scope_with(&self.root, resolve, stats)
    }

    /// [`Self::composite_scope`] body with injected resolver.
    fn composite_scope_with(
        &self,
        nodes: &[LayerNode],
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        stats: &mut super::compositing::CompositeStats,
    ) -> Option<DynamicImage> {
        let (half_w, half_h) = scope_half_extents(nodes, self.width, self.height, resolve);
        // Clamp to avoid OOM (16384 ≈ 1 GB RGBA)
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        let t_alloc = std::time::Instant::now();
        let mut acc = ImageBuffer::from_pixel(w.max(1), h.max(1), Rgba([0, 0, 0, 0]));
        stats.acc_alloc_us += t_alloc.elapsed().as_micros();
        stats.scope_px = u64::from(acc.width()) * u64::from(acc.height());
        // World origin (0,0) = buffer corner minus half-sizes
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        if !super::compositing::fold_scope_stats(
            nodes, &mut acc, origin_x, origin_y, resolve, stats,
        ) {
            return None; // no visible/contributing layer
        }
        Some(DynamicImage::ImageRgba8(acc))
    }

    /// REGIONAL (windowed) composite: only computes `region` (DOCUMENT
    /// coordinates, clamped here to the buffer) with EXACTLY the same
    /// pass as [`Self::composite_scope_with`] — same `prepare_top`,
    /// same `blend_into`, same full-size appearances — on a reduced
    /// accumulator with a shifted origin.
    ///
    /// Correctness: the shift is integer (buffer pixels), and integer
    /// translation commutes with `blend_into` rounding (`round(a − K)`
    /// == round(a) − K`) — so sampling is bit-identical to
    /// full-frame over the covered zone.
    ///
    /// Dependency window (Phase 6C): the stack is analyzed by
    /// [`super::compositing::scope_adjustment_window`] —
    /// - local/neighborhood ops: fall back to dependency `D = R + halo`
    ///   (halo = sum of exact supports, see
    ///   [`super::compositing::blur_support_px`]), then crop `R`;
    /// - global op (unknown effect, active group mask…): documented
    ///   fallback — full-frame evaluation then crop of `R` (correctness
    ///   first; `stats.scope_px` then reflects the full-frame
    ///   cost).
    ///
    /// Appearances stay computed full-size (no added GPU readback, no
    /// shader realloc). `stats.scope_px` = dependency pixels actually
    /// processed (window, not request). `None` if the region is
    /// empty/out of frame or nothing contributes.
    pub fn composite_region_with(
        &self,
        resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>,
        stats: &mut super::compositing::CompositeStats,
        region: &TileRegion,
    ) -> Option<RegionalComposite> {
        if region.is_empty() {
            return None;
        }
        // Same geometry as full-frame (no re-derivation from rounded
        // values — see `sample_color`).
        let (half_w, half_h) = scope_half_extents(&self.root, self.width, self.height, resolve);
        let w = ((half_w * 2.0).clamp(1.0, 16384.0)) as u32;
        let h = ((half_h * 2.0).clamp(1.0, 16384.0)) as u32;
        let origin_x = half_w - self.width as f32 / 2.0;
        let origin_y = half_h - self.height as f32 / 2.0;
        // Document → buffer, clamped integer bounds (floor at min, ceil at
        // max: conservative coverage like tiles).
        let bx0 = (origin_x + region.x as f32).floor().clamp(0.0, w as f32) as u32;
        let by0 = (origin_y + region.y as f32).floor().clamp(0.0, h as f32) as u32;
        let bx1 = (origin_x + region.x as f32 + region.width as f32)
            .ceil()
            .clamp(0.0, w as f32) as u32;
        let by1 = (origin_y + region.y as f32 + region.height as f32)
            .ceil()
            .clamp(0.0, h as f32) as u32;
        let (rw, rh) = (bx1.saturating_sub(bx0), by1.saturating_sub(by0));
        if rw == 0 || rh == 0 {
            return None;
        }
        let window = super::compositing::scope_adjustment_window(&self.root);
        if window.global {
            // Documented fallback: full-frame then crop — bit-identical by
            // construction, at full-frame cost.
            let full = self.composite_scope_with(&self.root, resolve, stats)?;
            let rgba = full.to_rgba8();
            let cropped = image::imageops::crop_imm(&rgba, bx0, by0, rw, rh).to_image();
            return Some(RegionalComposite {
                image: DynamicImage::ImageRgba8(cropped),
                origin_x: bx0,
                origin_y: by0,
            });
        }
        // D = R + halo (buffer coords, same step as the region), clamped. R
        // ⊆ D guaranteed (R already in the buffer): the final crop is safe.
        let halo = u64::from(window.halo_px);
        let dx0 = u64::from(bx0).saturating_sub(halo).min(u64::from(w)) as u32;
        let dy0 = u64::from(by0).saturating_sub(halo).min(u64::from(h)) as u32;
        let dx1 = (u64::from(bx1) + halo).min(u64::from(w)) as u32;
        let dy1 = (u64::from(by1) + halo).min(u64::from(h)) as u32;
        let (dw, dh) = (dx1.saturating_sub(dx0), dy1.saturating_sub(dy0));
        if dw == 0 || dh == 0 {
            return None;
        }
        let t_alloc = std::time::Instant::now();
        let mut acc = ImageBuffer::from_pixel(dw.max(1), dh.max(1), Rgba([0, 0, 0, 0]));
        stats.acc_alloc_us += t_alloc.elapsed().as_micros();
        stats.scope_px = u64::from(acc.width()) * u64::from(acc.height());
        if !super::compositing::fold_scope_stats(
            &self.root,
            &mut acc,
            origin_x - dx0 as f32,
            origin_y - dy0 as f32,
            resolve,
            stats,
        ) {
            return None;
        }
        let cropped = image::imageops::crop_imm(&acc, bx0 - dx0, by0 - dy0, rw, rh).to_image();
        Some(RegionalComposite {
            image: DynamicImage::ImageRgba8(cropped),
            origin_x: bx0,
            origin_y: by0,
        })
    }

    /// Composite CROPPED to document dimensions — used for export.
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

    /// Renderer statistics — test instrumentation (warm cache).
    #[cfg(test)]
    pub fn renderer_stats(&self) -> (u64, u64) {
        let r = self.cache.borrow();
        (r.hits(), r.misses())
    }

    /// Appearance counters (hits, misses, preview/thumb rebuilds) —
    /// inter-pass sharing observability, no render effect.
    pub fn appearance_stats(&self) -> crate::renderer::AppearanceStats {
        self.cache.borrow().stats()
    }
}

fn find_in(nodes: &[LayerNode], id: Uuid) -> Option<&LayerNode> {
    for n in nodes {
        if n.id() == id {
            return Some(n);
        }
        if let Some(found) = find_in(n.children(), id) {
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
        if let Some(children) = n.children_mut()
            && let Some(found) = find_in_mut(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// True if at least one node would contribute to the composite (same
/// guards as [`fold_scope`](super::compositing::fold_scope), no render).
/// A lone adjustment does not count: on an empty accumulator it
/// produces nothing, like the composite which then returns `None`.
/// A pixel layer also contributes via its clipped children.
fn contributes(nodes: &[LayerNode], resolve: &dyn Fn(Uuid) -> Option<Arc<DynamicImage>>) -> bool {
    for node in nodes {
        match node {
            LayerNode::Pixel(l) => {
                if l.visible && l.opacity > 0.01 && resolve(l.id).is_some() {
                    return true;
                }
                if l.visible && l.opacity > 0.01 && contributes(&l.children, resolve) {
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

/// Parent (`None` = root) and index of a node, for no-op detection.
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

/// Finds the list owning `id` (root or a group's children) + index.
fn find_owner_list(nodes: &mut Vec<LayerNode>, id: Uuid) -> Option<(&mut Vec<LayerNode>, usize)> {
    if let Some(idx) = nodes.iter().position(|n| n.id() == id) {
        return Some((nodes, idx));
    }
    for n in nodes.iter_mut() {
        if let Some(children) = n.children_mut()
            && let Some(found) = find_owner_list(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Recursively masks a subtree (dragging a group = the whole group).
fn hide_subtree(node: Option<&mut LayerNode>) {
    let Some(node) = node else { return };
    node.set_visible(false);
    if let Some(children) = node.children_mut() {
        for child in children.iter_mut() {
            hide_subtree(Some(child));
        }
    }
}

fn collect_pixels<'a>(nodes: &'a [LayerNode], out: &mut Vec<&'a PixelLayer>) {
    for n in nodes {
        match n {
            LayerNode::Pixel(l) => {
                if !l.visible || l.opacity <= 0.01 {
                    continue;
                }
                out.push(l);
                collect_pixels(&l.children, out);
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

/// Ids of all pixel layers, no visibility filter (snapshots need
/// thumbnails even of hidden layers). Descends the whole nested
/// hierarchy (groups AND pixel
/// children).
fn collect_all_pixel_ids(nodes: &[LayerNode], out: &mut Vec<Uuid>) {
    for n in nodes {
        if let LayerNode::Pixel(l) = n {
            out.push(l.id);
        }
        collect_all_pixel_ids(n.children(), out);
    }
}

fn collect_masks<'a>(
    nodes: &'a [LayerNode],
    out: &mut Vec<(Uuid, &'a crate::document::LayerMask)>,
) {
    for n in nodes {
        // The node's own masks (pixels, groups AND adjustments).
        for m in n.masks() {
            out.push((n.id(), m));
        }
        // Filter sub-layer masks (carrier = filter id).
        if let Some(filters) = n.filter_layers() {
            for f in filters {
                for m in &f.masks {
                    out.push((f.id, m));
                }
            }
        }
        collect_masks(n.children(), out)
    }
}

/// Filter sub-layer by its id (pixels, nested groups, root or
/// pixel children).
fn find_filter_in(nodes: &[LayerNode], filter_id: Uuid) -> Option<&FilterLayer> {
    for n in nodes {
        if let Some(filters) = n.filter_layers()
            && let Some(f) = filters.iter().find(|f| f.id == filter_id)
        {
            return Some(f);
        }
        if let Some(found) = find_filter_in(n.children(), filter_id) {
            return Some(found);
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
                if let Some(found) = find_filter_in_mut(&mut l.children, filter_id) {
                    return Some(found);
                }
            }
            LayerNode::Group(g) => {
                if let Some(filters) = g.filter_layers.iter_mut().find(|f| f.id == filter_id) {
                    return Some(filters);
                }
                if let Some(found) = find_filter_in_mut(&mut g.children, filter_id) {
                    return Some(found);
                }
            }
            LayerNode::Adjustment(_) => {}
        }
    }
    None
}

/// Carrier (pixels or group) of a filter sub-layer.
fn find_filter_parent_in(nodes: &[LayerNode], filter_id: Uuid) -> Option<Uuid> {
    for n in nodes {
        if n.filter_layers()
            .is_some_and(|filters| filters.iter().any(|f| f.id == filter_id))
        {
            return Some(n.id());
        }
        if let Some(found) = find_filter_parent_in(n.children(), filter_id) {
            return Some(found);
        }
    }
    None
}

/// Carrier of a mask by its id — a node (pixel/group/adjustment)
/// whose `masks` list holds it, OR the filter sub-layer carrying
/// it.
fn mask_owner_in(nodes: &[LayerNode], mask_id: Uuid) -> Option<Uuid> {
    for n in nodes {
        if n.masks().iter().any(|m| m.id == mask_id) {
            return Some(n.id());
        }
        if let Some(filters) = n.filter_layers() {
            for f in filters {
                if f.masks.iter().any(|m| m.id == mask_id) {
                    return Some(f.id);
                }
            }
        }
        if let Some(found) = mask_owner_in(n.children(), mask_id) {
            return Some(found);
        }
    }
    None
}
