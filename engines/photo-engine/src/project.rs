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

//! Native `.cygp` project format — UI-independent, reusable by the
//! suite's other apps (layer video compositing…).
//!
//! FORMAT_VERSION 5: hierarchical nesting (Affinity-style nesting) —
//! pixel layers' clipped children (`children`), groups' nested filters
//! (`filter_layers`), adjustments' masks (`masks`). Versioned JSON
//! container, PNG-then-base64 encoded images.
//!
//! Compatibility policy: v5 read natively, v4 read natively
//! (v5 fields absent = empty defaults), v3 MIGRATED (filters converted
//! to neutral sub-layers); any other version is refused with a clear
//! message.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use base64::Engine as _;
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::{
    AdjustmentLayer, BlendMode, Document, FilterLayer, FilterNode, GroupLayer, LayerNode,
    PixelLayer, Transform2D,
};

/// Format version — bump on any incompatible change. v5: nesting
/// (pixel children, group filters, adjustment masks). v4: pixel filters
/// = sub-layers (`filter_layers`) instead of `live_filters` (v3,
/// migrated on read).
pub const FORMAT_VERSION: u32 = 5;

/// Canonical Cygnus photo project extension.
pub const PROJECT_EXTENSION: &str = "cygp";

/// Old extensions still accepted FOR READING: `.csophoto`, then `.csphoto`
/// (pre-`cso*` convention).
pub const LEGACY_PROJECT_EXTENSIONS: [&str; 2] = ["csophoto", "csphoto"];

/// Does the file extension match a photo project (canonical `.cygp` or
/// legacy `.csophoto`/`.csphoto`)? Case-insensitive.
#[must_use]
pub fn is_project_path(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            ext == PROJECT_EXTENSION || LEGACY_PROJECT_EXTENSIONS.contains(&ext.as_str())
        }
        None => false,
    }
}

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    version: u32,
    width: u32,
    height: u32,
    root: Vec<LayerNodeDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LayerNodeDto {
    Pixel(PixelDto),
    Group(GroupDto),
    Adjustment(AdjustmentDto),
}

#[derive(Serialize, Deserialize)]
struct MaskDto {
    id: Uuid,
    /// Renamable in the Layers panel (default « Masque »).
    #[serde(default = "default_mask_name")]
    name: String,
    png_base64: String,
    enabled: bool,
    inverted: bool,
}

fn default_mask_name() -> String {
    String::from("Masque")
}

#[derive(Serialize, Deserialize)]
struct PixelDto {
    id: Uuid,
    name: String,
    /// SOURCE pixels (never the filtered appearance) PNG + base64 encoded.
    png_base64: String,
    /// LEGACY v3: plain filters, migrated to neutral sub-layers. Always
    /// written empty in v4+ (write), re-read for migration (read).
    #[serde(default)]
    live_filters: Vec<FilterDto>,
    /// v4: filter sub-layers (opacity/blend/transform/masks).
    #[serde(default)]
    filter_layers: Vec<FilterLayerDto>,
    /// v5: clipped children (Affinity-style nesting).
    #[serde(default)]
    children: Vec<LayerNodeDto>,
    transform: Transform2D,
    opacity: f32,
    blend_mode: BlendMode,
    visible: bool,
    #[serde(default)]
    masks: Vec<MaskDto>,
}

#[derive(Serialize, Deserialize)]
struct GroupDto {
    id: Uuid,
    name: String,
    children: Vec<LayerNodeDto>,
    /// v5: group nested filters (after children, before masks).
    #[serde(default)]
    filter_layers: Vec<FilterLayerDto>,
    #[serde(default)]
    collapsed: bool,
    opacity: f32,
    blend_mode: BlendMode,
    visible: bool,
    #[serde(default)]
    masks: Vec<MaskDto>,
}

#[derive(Serialize, Deserialize)]
struct AdjustmentDto {
    id: Uuid,
    name: String,
    filters: Vec<FilterDto>,
    opacity: f32,
    visible: bool,
    /// v5: masks restricting the effect zone.
    #[serde(default)]
    masks: Vec<MaskDto>,
}

#[derive(Serialize, Deserialize)]
struct FilterDto {
    id: Uuid,
    type_id: String,
    params: HashMap<String, datatypes::ParamValue>,
    enabled: bool,
}

impl FilterNode {
    fn to_dto(&self) -> FilterDto {
        FilterDto {
            id: self.id,
            type_id: self.type_id.clone(),
            params: self.params.clone(),
            enabled: self.enabled,
        }
    }

    fn from_dto(dto: FilterDto) -> Self {
        Self {
            id: dto.id,
            type_id: dto.type_id,
            params: dto.params,
            enabled: dto.enabled,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FilterLayerDto {
    id: Uuid,
    name: String,
    type_id: String,
    params: HashMap<String, datatypes::ParamValue>,
    enabled: bool,
    opacity: f32,
    blend_mode: BlendMode,
    transform: Transform2D,
    masks: Vec<MaskDto>,
}

impl FilterLayer {
    fn to_dto(&self, name: &str) -> Result<FilterLayerDto, String> {
        Ok(FilterLayerDto {
            id: self.id,
            name: self.name.clone(),
            type_id: self.type_id.clone(),
            params: self.params.clone(),
            enabled: self.enabled,
            opacity: self.opacity,
            blend_mode: self.blend_mode,
            transform: self.transform,
            masks: masks_to_dto(&self.masks, name)?,
        })
    }

    fn from_dto(dto: FilterLayerDto, name: &str) -> Result<Self, String> {
        Ok(Self {
            id: dto.id,
            name: dto.name,
            type_id: dto.type_id,
            params: dto.params,
            enabled: dto.enabled,
            opacity: dto.opacity.clamp(0.0, 100.0),
            blend_mode: dto.blend_mode,
            transform: sanitize_transform(dto.transform),
            masks: masks_from_dto(dto.masks, name)?,
        })
    }

    /// v3 → v4 migration: a plain filter becomes a neutral sub-layer (same
    /// effects/parameters/state, default layer attributes).
    fn migrate_v3(dto: FilterDto) -> Self {
        let name = dto.type_id.clone();
        Self {
            id: dto.id,
            name,
            type_id: dto.type_id,
            params: dto.params,
            enabled: dto.enabled,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            transform: Transform2D::default(),
            masks: Vec::new(),
        }
    }
}

fn png_encode(img: &DynamicImage, name: &str) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("Encodage PNG du calque « {name} » : {e}"))?;
    Ok(png)
}

fn png_decode(png_base64: &str, name: &str) -> Result<DynamicImage, String> {
    let png = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| format!("Calque « {name} » corrompu : {e}"))?;
    image::load_from_memory(&png).map_err(|e| format!("Calque « {name} » illisible : {e}"))
}

fn masks_to_dto(masks: &[crate::document::LayerMask], name: &str) -> Result<Vec<MaskDto>, String> {
    masks.iter().map(|m| mask_to_dto(m, name)).collect()
}

fn mask_to_dto(mask: &crate::document::LayerMask, name: &str) -> Result<MaskDto, String> {
    let dyn_img = image::DynamicImage::ImageRgba8((*mask.image).clone());
    Ok(MaskDto {
        id: mask.id,
        name: mask.name.clone(),
        png_base64: base64::engine::general_purpose::STANDARD.encode(png_encode(&dyn_img, name)?),
        enabled: mask.enabled,
        inverted: mask.inverted,
    })
}

fn masks_from_dto(
    dtos: Vec<MaskDto>,
    name: &str,
) -> Result<Vec<crate::document::LayerMask>, String> {
    dtos.into_iter().map(|m| mask_from_dto(m, name)).collect()
}

fn mask_from_dto(dto: MaskDto, name: &str) -> Result<crate::document::LayerMask, String> {
    let img = png_decode(&dto.png_base64, name)?;
    Ok(crate::document::LayerMask {
        id: dto.id,
        name: dto.name,
        image: std::sync::Arc::new(img.to_rgba8()),
        enabled: dto.enabled,
        inverted: dto.inverted,
        version: crate::document::next_appearance_version(),
    })
}

fn node_to_dto(node: &LayerNode) -> Result<LayerNodeDto, String> {
    Ok(match node {
        LayerNode::Pixel(l) => {
            // SOURCE encoding: filters stay live on reload
            let children: Result<Vec<LayerNodeDto>, String> =
                l.children.iter().map(node_to_dto).collect();
            LayerNodeDto::Pixel(PixelDto {
                id: l.id,
                name: l.name.clone(),
                png_base64: base64::engine::general_purpose::STANDARD
                    .encode(png_encode(l.source_image.as_ref(), &l.name)?),
                live_filters: Vec::new(),
                filter_layers: l
                    .filter_layers
                    .iter()
                    .map(|f| f.to_dto(&l.name))
                    .collect::<Result<Vec<_>, _>>()?,
                children: children?,
                transform: l.transform,
                opacity: l.opacity,
                blend_mode: l.blend_mode,
                visible: l.visible,
                masks: masks_to_dto(&l.masks, &l.name)?,
            })
        }
        LayerNode::Group(g) => {
            let children: Result<Vec<LayerNodeDto>, String> =
                g.children.iter().map(node_to_dto).collect();
            LayerNodeDto::Group(GroupDto {
                id: g.id,
                name: g.name.clone(),
                children: children?,
                filter_layers: g
                    .filter_layers
                    .iter()
                    .map(|f| f.to_dto(&g.name))
                    .collect::<Result<Vec<_>, _>>()?,
                collapsed: g.collapsed,
                opacity: g.opacity,
                blend_mode: g.blend_mode,
                visible: g.visible,
                masks: masks_to_dto(&g.masks, &g.name)?,
            })
        }
        LayerNode::Adjustment(a) => LayerNodeDto::Adjustment(AdjustmentDto {
            id: a.id,
            name: a.name.clone(),
            filters: a.filters.iter().map(FilterNode::to_dto).collect(),
            opacity: a.opacity,
            visible: a.visible,
            masks: masks_to_dto(&a.masks, &a.name)?,
        }),
    })
}

fn sanitize_transform(t: Transform2D) -> Transform2D {
    Transform2D {
        offset_x: t.offset_x,
        offset_y: t.offset_y,
        rotation_deg: t.rotation_deg,
        scale_x: t.scale_x.clamp(0.05, 8.0),
        scale_y: t.scale_y.clamp(0.05, 8.0),
        skew_x: t.skew_x.clamp(-80.0, 80.0),
        skew_y: t.skew_y.clamp(-80.0, 80.0),
    }
}

fn node_from_dto(dto: LayerNodeDto, legacy_v3: bool) -> Result<LayerNode, String> {
    Ok(match dto {
        LayerNodeDto::Pixel(p) => {
            let img = png_decode(&p.png_base64, &p.name)?;
            let mut layer = PixelLayer::new(p.name.clone(), Arc::new(img));
            layer.id = p.id;
            layer.filter_layers = if legacy_v3 {
                p.live_filters
                    .into_iter()
                    .map(FilterLayer::migrate_v3)
                    .collect()
            } else {
                p.filter_layers
                    .into_iter()
                    .map(|f| FilterLayer::from_dto(f, &p.name))
                    .collect::<Result<Vec<_>, _>>()?
            };
            // v5: clipped children (absent in v3/v4 → `#[serde(default)]`).
            let children: Result<Vec<LayerNode>, String> = p
                .children
                .into_iter()
                .map(|c| node_from_dto(c, legacy_v3))
                .collect();
            layer.children = children?;
            layer.transform = sanitize_transform(p.transform);
            layer.opacity = p.opacity.clamp(0.0, 100.0);
            layer.blend_mode = p.blend_mode;
            layer.visible = p.visible;
            layer.masks = masks_from_dto(p.masks, &p.name)?;
            LayerNode::Pixel(layer)
        }
        LayerNodeDto::Group(g) => {
            let children: Result<Vec<LayerNode>, String> = g
                .children
                .into_iter()
                .map(|c| node_from_dto(c, legacy_v3))
                .collect();
            let mut group = GroupLayer::new(g.name.clone(), children?);
            group.id = g.id;
            group.filter_layers = g
                .filter_layers
                .into_iter()
                .map(|f| FilterLayer::from_dto(f, &g.name))
                .collect::<Result<Vec<_>, _>>()?;
            group.collapsed = g.collapsed;
            group.opacity = g.opacity.clamp(0.0, 100.0);
            group.blend_mode = g.blend_mode;
            group.visible = g.visible;
            group.masks = masks_from_dto(g.masks, &g.name)?;
            LayerNode::Group(group)
        }
        LayerNodeDto::Adjustment(a) => {
            let masks = masks_from_dto(a.masks, &a.name)?;
            let mut adj = AdjustmentLayer::new(
                a.name,
                a.filters.into_iter().map(FilterNode::from_dto).collect(),
            );
            adj.id = a.id;
            adj.opacity = a.opacity.clamp(0.0, 100.0);
            adj.visible = a.visible;
            adj.masks = masks;
            LayerNode::Adjustment(adj)
        }
    })
}

/// Document reloaded from a `.cygp`.
pub struct LoadedProject {
    pub document: Document,
    /// Loaded file path (becomes the current save path)
    pub path: Option<std::path::PathBuf>,
    /// File name without extension — feeds the canvas title
    pub source_name: Option<String>,
}

impl Clone for LoadedProject {
    fn clone(&self) -> Self {
        let mut document = Document::new(self.document.width, self.document.height);
        document.restore_snapshot(self.document.snapshot());
        Self {
            document,
            path: self.path.clone(),
            source_name: self.source_name.clone(),
        }
    }
}

impl std::fmt::Debug for LoadedProject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedProject")
            .field("width", &self.document.width)
            .field("height", &self.document.height)
            .field("root", &self.document.root.len())
            .field("path", &self.path)
            .field("source_name", &self.source_name)
            .finish()
    }
}

/// Saves the layer tree to a `.cygp` file (v5).
///
/// # Errors
/// PNG encoding error of a layer, JSON serialization or disk write —
/// descriptive message in French for the user.
pub fn save(path: &Path, doc: &Document) -> Result<(), String> {
    let root: Result<Vec<LayerNodeDto>, String> = doc.root.iter().map(node_to_dto).collect();
    let file = ProjectFile {
        version: FORMAT_VERSION,
        width: doc.width,
        height: doc.height,
        root: root?,
    };
    let json =
        serde_json::to_vec_pretty(&file).map_err(|e| format!("Sérialisation du projet : {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Écriture de {}: {e}", path.display()))
}

/// Loads a `.cygp`. v5 read natively, v4 read natively (v5 fields absent
/// = defaults), v3 migrated (plain filters → neutral sub-layers); any
/// other version is refused.
///
/// # Errors
/// Unreadable file, invalid JSON, foreign version, or corrupted layer
/// — descriptive message in French.
pub fn load(path: &Path) -> Result<LoadedProject, String> {
    let json = std::fs::read(path).map_err(|e| format!("Lecture de {}: {e}", path.display()))?;
    // Version probe BEFORE full deserialization: a project from another
    // era must be refused with the right message, not a missing-fields
    // error.
    #[derive(Deserialize)]
    struct VersionProbe {
        version: u32,
    }
    let probe: VersionProbe =
        serde_json::from_slice(&json).map_err(|e| format!("Projet invalide : {e}"))?;
    let legacy_v3 = match probe.version {
        FORMAT_VERSION | 4 => false,
        3 => true,
        v => {
            return Err(format!(
                "Version de projet non supportée : {v} (v3 migrée, v4 et v5 natives). \
                 Les projets au format 1 et 2 ne sont plus lisibles.",
            ));
        }
    };
    let file: ProjectFile =
        serde_json::from_slice(&json).map_err(|e| format!("Projet invalide : {e}"))?;

    let root: Result<Vec<LayerNode>, String> = file
        .root
        .into_iter()
        .map(|n| node_from_dto(n, legacy_v3))
        .collect();
    let root = root?;

    let source_name = path
        .file_stem()
        .and_then(|n| n.to_str())
        .map(str::to_string);

    let mut document = Document::new(file.width.max(1), file.height.max(1));
    document.restore(file.width, file.height, root);

    Ok(LoadedProject {
        document,
        path: Some(path.to_path_buf()),
        source_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use datatypes::ParamValue;
    use image::RgbaImage;

    fn red_img() -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8({
            let mut b = RgbaImage::new(3, 2);
            b.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
            b
        }))
    }

    fn green_img() -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8({
            let mut b = RgbaImage::new(2, 2);
            b.put_pixel(1, 1, image::Rgba([0, 255, 0, 255]));
            b
        }))
    }

    /// Root: [Group(top, filtered background), Adjustment(color_correct)]
    fn sample_document() -> Document {
        let mut doc = Document::new(800, 600);
        let mut haut = PixelLayer::new("haut", green_img());
        haut.transform.offset_y = -4.0;

        let mut fond = PixelLayer::new("fond", red_img());
        fond.opacity = 75.0;
        fond.transform.offset_x = 12.5;
        let mut filtre = FilterLayer::neutral("brightness_contrast", Default::default());
        filtre
            .params
            .insert("brightness".into(), ParamValue::Float(25.0));
        fond.filter_layers.push(filtre);

        doc.push_layer(LayerNode::Group(GroupLayer::new(
            "groupe",
            vec![LayerNode::Pixel(haut), LayerNode::Pixel(fond)],
        )));
        doc.push_layer(LayerNode::Adjustment(AdjustmentLayer::new(
            "courbe",
            vec![FilterNode::new("color_correct")],
        )));
        doc
    }

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        std::env::temp_dir().join(format!("cyg-{tag}-{}.cygp", nanos))
    }

    #[test]
    fn aller_retour_projet_v4_conserve_arbre_et_filtres() {
        let doc = sample_document();
        let path = temp_path("rt");
        save(&path, &doc).expect("sauvegarde");

        let loaded = load(&path).expect("chargement");
        std::fs::remove_file(&path).ok();

        assert_eq!((loaded.document.width, loaded.document.height), (800, 600));
        // Root: [Group(pixels ×2), Adjustment]
        assert_eq!(loaded.document.root.len(), 2);
        let Some(LayerNode::Group(g)) = loaded.document.root.first() else {
            panic!("groupe attendu à la racine");
        };
        assert_eq!(g.name, "groupe");
        assert_eq!(g.children.len(), 2);

        let LayerNode::Pixel(fond) = &g.children[1] else {
            panic!("calque pixels attendu");
        };
        assert_eq!(fond.id.to_string().len(), 36, "uuid préservé");
        assert_eq!(fond.opacity, 75.0);
        assert_eq!(fond.transform.offset_x, 12.5);
        assert_eq!(fond.blend_mode, BlendMode::Normal);
        assert_eq!(fond.filter_layers.len(), 1);
        assert_eq!(fond.filter_layers[0].type_id, "brightness_contrast");
        assert_eq!(
            fond.filter_layers[0].params.get("brightness"),
            Some(&ParamValue::Float(25.0)),
            "paramètres de filtre préservés"
        );
        // Identical pixels after the PNG round-trip
        let reference = sample_document();
        let Some(LayerNode::Group(gr)) = reference.root.first() else {
            panic!()
        };
        let LayerNode::Pixel(fond_ref) = &gr.children[1] else {
            panic!()
        };
        assert_eq!(*fond.source_image, *fond_ref.source_image);
    }

    #[test]
    fn detection_extension_projet_canonique_et_heritee() {
        assert!(is_project_path(Path::new("mon-projet.cygp")));
        assert!(is_project_path(Path::new("MON-PROJET.CYGP")));
        // Old extensions still recognized for reading
        assert!(is_project_path(Path::new("ancien.csophoto")));
        assert!(is_project_path(Path::new("ancien.csphoto")));
        assert!(!is_project_path(Path::new("image.png")));
        assert!(!is_project_path(Path::new("sans-extension")));
    }

    #[test]
    fn version_etrangere_rejetee_proprement() {
        let path = temp_path("bad");
        std::fs::write(&path, r#"{"version":1,"width":4,"height":4,"layers":[]}"#)
            .expect("écriture fixture v1");
        let err = load(&path).expect_err("v1 doit être refusée");
        assert!(err.contains("non supportée"), "{err}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn projet_v3_migre_en_sous_calques_neutres() {
        // REAL v3 fixture: valid PNG + live_filters → migrated to sub-layers
        // (effect/params/state kept, neutral attributes).
        let png = png_encode(
            &DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([100, 100, 100, 255]),
            )),
            "fond",
        )
        .expect("encodage fixture");
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let json = format!(
            r#"{{"version":3,"width":2,"height":2,"root":[
                {{"kind":"pixel","id":"12345678-1234-1234-1234-1234567890ab",
                 "name":"fond","png_base64":"{b64}",
                 "live_filters":[{{"id":"12345678-1234-1234-1234-1234567890ac",
                   "type_id":"brightness_contrast",
                   "params":{{"brightness":{{"Float":25.0}}}},"enabled":true}}],
                 "transform":{{"offset_x":0.0,"offset_y":0.0,"rotation_deg":0.0,
                   "scale_x":1.0,"scale_y":1.0,"skew_x":0.0,"skew_y":0.0}},
                 "opacity":100.0,"blend_mode":"Normal","visible":true}}
            ]}}"#
        );
        let path = temp_path("v3");
        std::fs::write(&path, json).expect("écriture fixture v3");
        let loaded = load(&path).expect("v3 doit être migrée, pas refusée");
        std::fs::remove_file(&path).ok();

        let LayerNode::Pixel(fond) = &loaded.document.root[0] else {
            panic!("pixel attendu");
        };
        assert_eq!(fond.filter_layers.len(), 1);
        let f = &fond.filter_layers[0];
        assert_eq!(f.type_id, "brightness_contrast");
        assert!(f.enabled);
        assert_eq!(f.opacity, 100.0);
        assert_eq!(f.params.get("brightness"), Some(&ParamValue::Float(25.0)));
        // And rendering follows: 100 + 25*2.55 ≈ 163.
        let appearance = loaded.document.appearance(fond.id).expect("apparence");
        let px = appearance.image.to_rgba8().get_pixel(0, 0).0;
        assert!((px[0] as f32 - 163.0).abs() <= 3.0, "{px:?}");
    }

    #[test]
    fn migration_v3_conserve_effets_et_params() {
        // real v3: saved via legacy DTOs then reloaded.
        let mut doc = Document::new(2, 2);
        let mut fond = PixelLayer::new("fond", red_img());
        let legacy = FilterDto {
            id: uuid::Uuid::new_v4(),
            type_id: "brightness_contrast".into(),
            params: [("brightness".to_string(), ParamValue::Float(25.0))]
                .into_iter()
                .collect(),
            enabled: false,
        };
        fond.filter_layers.push(FilterLayer::migrate_v3(legacy));
        doc.push_layer(LayerNode::Pixel(fond));
        let path = temp_path("v3mig");
        save(&path, &doc).expect("sauvegarde");
        let loaded = load(&path).expect("chargement");
        std::fs::remove_file(&path).ok();
        let LayerNode::Pixel(fond) = &loaded.document.root[0] else {
            panic!("pixel attendu");
        };
        assert_eq!(fond.filter_layers.len(), 1);
        let f = &fond.filter_layers[0];
        assert_eq!(f.type_id, "brightness_contrast");
        assert!(!f.enabled);
        assert_eq!(f.opacity, 100.0);
        assert_eq!(f.params.get("brightness"), Some(&ParamValue::Float(25.0)));
    }

    #[test]
    fn apparence_regeneree_apres_chargement() {
        let doc = sample_document();
        let path = temp_path("appear");
        save(&path, &doc).expect("sauvegarde");
        let loaded = load(&path).expect("chargement");
        std::fs::remove_file(&path).ok();

        let Some(LayerNode::Group(g)) = loaded.document.root.first() else {
            panic!()
        };
        let LayerNode::Pixel(fond) = &g.children[1] else {
            panic!()
        };
        let id = fond.id;
        let appearance = loaded.document.appearance(id).expect("apparence");
        // source-dimensioned preview, standard 48 px thumb
        assert_eq!(
            (appearance.preview.width, appearance.preview.height),
            (3, 2)
        );
        assert_eq!(appearance.thumb.width, 48);
        // The adjustment is present and active
        assert!(loaded.document.needs_fallback());
    }

    #[test]
    fn aller_retour_masque_conserve_pixels_et_flags() {
        let mut doc = Document::new(4, 4);
        let img = Arc::new(DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([10, 20, 30, 255]),
        )));
        let mut layer = PixelLayer::new("masqué", img);
        let mut mask = crate::document::LayerMask::full(2, 2);
        mask.enabled = false;
        mask.inverted = true;
        // distinct pixel to verify PNG
        mask.image = std::sync::Arc::new(RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255])));
        layer.masks.push(mask);
        doc.push_layer(LayerNode::Pixel(layer));
        let path = temp_path("mask");
        save(&path, &doc).expect("save masque");
        let loaded = load(&path).expect("load masque");
        std::fs::remove_file(&path).ok();
        let Some(LayerNode::Pixel(l)) = loaded.document.root.first() else {
            panic!("pixel attendu");
        };
        let m = l.masks.first().expect("masque préservé");
        assert!(!m.enabled);
        assert!(m.inverted);
        assert_eq!(m.image.get_pixel(0, 0)[0], 0);
    }

    /// Nested document: clipped child + group filter + adjustment mask
    /// (full v5 nesting workout).
    fn nested_doc() -> Document {
        let mut doc = Document::new(4, 4);
        let mut parent = PixelLayer::new("parent", red_img());
        parent
            .children
            .push(LayerNode::Pixel(PixelLayer::new("enfant", green_img())));
        let pid = parent.id;
        doc.push_layer(LayerNode::Pixel(parent));
        let mut groupe = GroupLayer::new(
            "g",
            vec![LayerNode::Pixel(PixelLayer::new("dedans", green_img()))],
        );
        let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
        f.params
            .insert("brightness".into(), ParamValue::Float(10.0));
        groupe.filter_layers.push(f);
        doc.push_layer(LayerNode::Group(groupe));
        let mut adj = AdjustmentLayer::new("aj", vec![]);
        adj.masks.push(crate::document::LayerMask::full(4, 4));
        doc.push_layer(LayerNode::Adjustment(adj));
        let _ = pid;
        doc
    }

    #[test]
    fn roundtrip_v5_nesting_complet() {
        let doc = nested_doc();
        let path = temp_path("v5nest");
        save(&path, &doc).expect("sauvegarde v5");
        // Written version = 5.
        let raw = std::fs::read(&path).expect("relecture");
        let probe: serde_json::Value = serde_json::from_slice(&raw).expect("json");
        assert_eq!(probe["version"], 5);
        let loaded = load(&path).expect("chargement v5");
        std::fs::remove_file(&path).ok();
        // Nested structure preserved (stable ids).
        let LayerNode::Pixel(parent) = &loaded.document.root[0] else {
            panic!("parent attendu");
        };
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name(), "enfant");
        let LayerNode::Group(groupe) = &loaded.document.root[1] else {
            panic!("groupe attendu");
        };
        assert_eq!(groupe.filter_layers.len(), 1);
        assert_eq!(groupe.filter_layers[0].type_id, "brightness_contrast");
        assert_eq!(
            groupe.filter_layers[0].params.get("brightness"),
            Some(&ParamValue::Float(10.0))
        );
        let LayerNode::Adjustment(adj) = &loaded.document.root[2] else {
            panic!("ajustement attendu");
        };
        assert_eq!(adj.masks.len(), 1);
        // Identical render before/after (nesting pixels).
        let before = doc.composite_preview().expect("composite avant");
        let after = loaded
            .document
            .composite_preview()
            .expect("composite après");
        assert_eq!(before.to_rgba8().as_raw(), after.to_rgba8().as_raw());
    }

    /// Recursively removes v5 keys from a project JSON (simulates a v4-
    /// written file): pixels' `children`, groups' `filter_layers`,
    /// adjustments' `masks`. v4 keys (including groups' `children`) are
    /// kept.
    fn strip_v5(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                match map.get("kind").and_then(|k| k.as_str()) {
                    Some("pixel") => {
                        map.remove("children");
                    }
                    Some("group") => {
                        map.remove("filter_layers");
                    }
                    Some("adjustment") => {
                        map.remove("masks");
                    }
                    _ => {}
                }
                for v in map.values_mut() {
                    strip_v5(v);
                }
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    strip_v5(v);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn projet_v4_charge_champs_v5_vides() {
        // Real v5 save, downgraded to v4 (version + v5 field removal):
        // lossless loading, empty fields.
        let doc = nested_doc();
        let path = temp_path("v4nest");
        save(&path, &doc).expect("sauvegarde v5");
        let raw = std::fs::read(&path).expect("relecture");
        let mut v4: serde_json::Value = serde_json::from_slice(&raw).expect("json");
        v4["version"] = serde_json::Value::from(4);
        strip_v5(&mut v4);
        std::fs::write(&path, serde_json::to_vec(&v4).expect("json v4")).expect("écriture v4");
        let loaded = load(&path).expect("v4 doit charger (champs v5 par défaut)");
        std::fs::remove_file(&path).ok();
        // v4 content intact (pixels, pixel filters, name).
        let LayerNode::Pixel(parent) = &loaded.document.root[0] else {
            panic!("parent attendu");
        };
        assert_eq!(parent.name, "parent");
        assert!(parent.children.is_empty(), "pas d'enfants en v4");
        let LayerNode::Group(groupe) = &loaded.document.root[1] else {
            panic!("groupe attendu");
        };
        assert_eq!(groupe.children.len(), 1);
        assert!(
            groupe.filter_layers.is_empty(),
            "pas de filtres de groupe en v4"
        );
        let LayerNode::Adjustment(adj) = &loaded.document.root[2] else {
            panic!("ajustement attendu");
        };
        assert!(adj.masks.is_empty(), "pas de masques d'ajustement en v4");
        // And v4 rendering stays correct (no nesting).
        assert!(loaded.document.composite_preview().is_some());
    }
}
