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

//! Modèle document « LayerTree » (arbre hiérarchique, style Affinity) +
//! compositing rapide.
//!
//! TODO chantier 4: fichier god-file (~2300 lignes) à découper en
//! `document/model.rs` + `document/tree.rs` + `document/compositing.rs` + tests
//! locaux. À faire juste avant la prochaine feature majeure (masques déjà livrés,
//! donc découpage à planifier en PR dédiée — aucune API externe ne doit changer).
//!
//! Architecture hybride :
//! - [`Document`] possède un arbre de [`LayerNode`] : calques pixels,
//!   groupes et calques d'ajustement, imbriqués à volonté.
//! - Chaque [`PixelLayer`] garde son image SOURCE intacte ; les retouches
//!   vivent dans des sous-calques [`FilterLayer`] (façon Affinity)
//!   évalués séquentiellement (voir [`crate::filters`]).
//! - L'apparence dérivée (source × filtres) est calculée paresseusement et
//!   mise en cache par version d'apparence : un réglage ne recalcule que la
//!   chaîne du calque concerné, l'undo/redo ne recalcule que si nécessaire.
//!
//! Conçu pour l'interaction temps réel, comme les éditeurs pro :
//! - fusion DIRECTE dans un buffer accumulateur (aucune copie intermédiaire)
//! - chemin CPU rayon RGBA8 : la fusion est memory-bound, le round-trip GPU
//!   coûtait plus cher que le calcul lui-même — le GPU reste réservé aux
//!   filtres lourds (blur…)
//!
//! Ce module est PUR : aucun framework UI. Les aperçus/miniatures sont des
//! [`RgbaBuf`] partageables sans copie ; l'app les convertit en textures.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tampon RGBA8 partageable avec l'UI SANS copie (l'app en dérive ses
/// textures via `Bytes::from_owner` sur l'Arc).
#[derive(Clone)]
pub struct RgbaBuf {
    pub width: u32,
    pub height: u32,
    pub data: Arc<[u8]>,
}

impl RgbaBuf {
    /// Create a shareable RGBA buffer from raw bytes.
    #[must_use]
    pub fn from_vec(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data: data.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Types de base du modèle
// ---------------------------------------------------------------------------

/// Mode de fusion d'un calque ou d'un groupe.
///
/// La représentation sérialisée est le nom de la variante (« Normal »,
/// « Multiply »…) — identique aux libellés historiques de l'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
}

impl std::fmt::Display for BlendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl BlendMode {
    /// Ordre d'affichage dans l'UI (listes déroulantes).
    pub const ALL: [BlendMode; 6] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
    ];

    /// Human-readable label for UI.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
        }
    }

    /// Identifiant numérique (shader GPU + CPU). Doit rester aligné sur
    /// `SHADER_BLEND` (gpu.rs) et les tests golden.
    #[must_use]
    pub fn id(self) -> u32 {
        match self {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
            BlendMode::Darken => 4,
            BlendMode::Lighten => 5,
        }
    }
}

/// Transformation affine 2D : définition canonique dans `math-utils`
/// (partagée avec `ui-kit` qui ne peut pas dépendre des engines).
/// Ré-exportée ici pour ne pas casser les `use crate::document::Transform2D`.
pub use math_utils::Transform2D;

/// Un filtre dynamique (live filter) : référence nommée vers un effet du
/// registre + ses paramètres. Utilisé TEL QUEL par les calques
/// d'ajustement ; les calques pixels portent des [`FilterLayer`]
/// (sous-calques à part entière, façon Affinity).
#[derive(Debug, Clone)]
pub struct FilterNode {
    pub id: Uuid,
    /// type_id du registre d'effets (ex. « brightness_contrast », « blur »)
    pub type_id: String,
    pub params: HashMap<String, datatypes::ParamValue>,
    pub enabled: bool,
}

impl FilterNode {
    /// Create a new enabled filter with empty params.
    pub fn new(type_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            type_id: type_id.into(),
            params: HashMap::new(),
            enabled: true,
        }
    }
}

/// Un filtre vivant comme SOUS-CALQUE d'un calque pixels (façon Affinity :
/// live filters imbriqués, réordonnables, chacun avec opacité, fusion,
/// transformation et masques propres). L'effet (`type_id` + `params`)
/// s'applique au résultat accumulé en dessous de lui dans la chaîne du
/// parent, puis est composité avec ses attributs de calque.
///
/// L'ordre du `Vec` parent = ordre d'application (index 0 = premier).
#[derive(Debug, Clone)]
pub struct FilterLayer {
    /// Identifiant stable (sélection, historique, projet).
    pub id: Uuid,
    /// Nom d'affichage (défaut = nom de la définition, renommable).
    pub name: String,
    /// type_id du registre d'effets (ex. « brightness_contrast », « blur »)
    pub type_id: String,
    pub params: HashMap<String, datatypes::ParamValue>,
    /// Interrupteur d'effet (œil du panneau Calques) — désactivé =
    /// transparent, réglages conservés.
    pub enabled: bool,
    /// 0..=100 — pondération du résultat filtré sur l'accumulé.
    pub opacity: f32,
    pub blend_mode: BlendMode,
    /// Espace du parent (identité par défaut — comme un filtre imbriqué
    /// Affinity qui suit son calque).
    pub transform: Transform2D,
    /// Masques de couverture — fusionnés multiplicativement dans l'espace
    /// du parent, AVANT la transform du sous-calque (même espace que la
    /// peinture, qui utilise la transform du porteur).
    pub masks: Vec<LayerMask>,
}

impl FilterLayer {
    /// Nouveau sous-calque avec les paramètres PAR DÉFAUT de sa définition.
    pub fn new(
        type_id: impl Into<String>,
        name: impl Into<String>,
        params: HashMap<String, datatypes::ParamValue>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            type_id: type_id.into(),
            params,
            enabled: true,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            transform: Transform2D::default(),
            masks: Vec::new(),
        }
    }

    /// Sous-calque « neutre » : opacité pleine, fusion normale, sans
    /// transform ni masque — rendu strictement identique à l'ancien pli.
    pub fn neutral(
        type_id: impl Into<String>,
        params: HashMap<String, datatypes::ParamValue>,
    ) -> Self {
        let type_id = type_id.into();
        Self::new(type_id.clone(), type_id, params)
    }

    /// Sous-calque reconstitué depuis un filtre simple (ajustements) —
    /// attributs de calque neutres, identité (id, état) préservée.
    pub fn from_node(node: FilterNode) -> Self {
        Self {
            id: node.id,
            name: node.type_id.clone(),
            type_id: node.type_id,
            params: node.params,
            enabled: node.enabled,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            transform: Transform2D::default(),
            masks: Vec::new(),
        }
    }

    /// Chemin rapide : aucun attribut de calque actif — le résultat de
    /// l'effet remplace l'accumulé tel quel (zéro blend, zéro allocation).
    #[must_use]
    pub fn is_passthrough(&self) -> bool {
        (self.opacity - 100.0).abs() < 0.01
            && self.blend_mode == BlendMode::Normal
            && self.transform == Transform2D::default()
            && !self.masks.iter().any(|m| m.enabled)
    }
}

/// Compteur global monotone des versions d'apparence : garantit qu'un numéro
/// ne désigne jamais deux contenus différents (undo/redo inclus).
static APPEARANCE_VERSION: AtomicU64 = AtomicU64::new(1);

pub fn next_appearance_version() -> u64 {
    APPEARANCE_VERSION.fetch_add(1, Ordering::Relaxed)
}

/// Masque raster non destructif attaché à un PixelLayer, un GroupLayer ou
/// un sous-calque de filtre (dans ce dernier cas : espace du calque parent).
/// Vit dans le MÊME espace que son porteur (dimensions du porteur, suit sa
/// transform) — comme le masque « lié » par défaut d'Affinity/Photoshop.
/// Stocké en RGBA8 (R=G=B=couverture, A=255) pour réutiliser
/// `paint::paint_stroke_rgba` sans dupliquer la rastérisation.
/// TODO v2: Luma8 pur (÷4 mémoire), feather/flou dédié, unlink transform.
#[derive(Clone, Debug)]
pub struct LayerMask {
    /// Identifiant stable, utilisé par l'app pour sélectionner / éditer /
    /// supprimer CE masque parmi les N masques d'un calque.
    pub id: Uuid,
    /// Nom d'affichage (défaut « Masque N », renommable façon calque).
    pub name: String,
    /// Buffer de couverture RGBA8 (R = couverture, A = 255 constant).
    pub image: Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>,
    pub enabled: bool,
    pub inverted: bool,
    pub version: u64,
}

impl LayerMask {
    /// Masque plein blanc (tout visible) aux dimensions données.
    #[must_use]
    pub fn full(width: u32, height: u32) -> Self {
        let buf = ImageBuffer::from_pixel(width, height, Rgba([255, 255, 255, 255]));
        Self {
            id: Uuid::new_v4(),
            name: String::from("Masque"),
            image: Arc::new(buf),
            enabled: true,
            inverted: false,
            version: next_appearance_version(),
        }
    }

    pub fn touch(&mut self) {
        self.version = next_appearance_version();
    }

    /// Couverture 0.0..=1.0 au pixel (x,y), en tenant compte de `inverted`.
    /// L'appelant doit vérifier `enabled` avant (retourne 1.0 si désactivé).
    #[inline]
    #[must_use]
    pub fn coverage_at(&self, x: u32, y: u32) -> f32 {
        let c = self.image.get_pixel(x, y)[0] as f32 / 255.0;
        if self.inverted { 1.0 - c } else { c }
    }
}

/// Calque pixels : image SOURCE non destructive + sous-calques de filtres.
///
/// L'image source ne change que lors d'événements explicites (peinture,
/// import, édition destructive volontaire) ; tout le reste vit dans les
/// filtres ou la transformation.
#[derive(Clone, Debug)]
pub struct PixelLayer {
    pub id: Uuid,
    pub name: String,
    pub source_image: Arc<DynamicImage>,
    /// Sous-calques de filtres non destructifs (façon Affinity — ordre =
    /// ordre d'application, index 0 = premier).
    pub filter_layers: Vec<FilterLayer>,
    pub transform: Transform2D,
    /// 0..=100
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    /// Masques de couverture — fusionnés multiplicativement dans le compositing.
    pub masks: Vec<LayerMask>,
    /// Incrémenté à TOUTE mutation affectant l'apparence (source ou filtres).
    /// Clé d'invalidation du cache d'apparence du document.
    pub appearance_version: u64,
}

impl PixelLayer {
    /// Create a new pixel layer with given name and source image.
    pub fn new(name: impl Into<String>, image: Arc<DynamicImage>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source_image: image,
            filter_layers: Vec::new(),
            transform: Transform2D::default(),
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            masks: Vec::new(),
            appearance_version: next_appearance_version(),
        }
    }

    /// Source dimensions in pixels.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        self.source_image.dimensions()
    }

    /// Bump la version d'apparence (à appeler après toute mutation pixels/filtres)
    pub fn touch(&mut self) {
        self.appearance_version = next_appearance_version();
    }

    /// Remplace le contenu pixels (peinture, édition destructive…)
    pub fn set_source_image(&mut self, image: DynamicImage) {
        self.source_image = Arc::new(image);
        self.touch();
    }
}

/// Groupe de calques : ses enfants composent entre eux d'abord, puis le
/// résultat est fondu dans le parent avec l'opacité/mode DU GROUPE.
/// Les transforms des enfants restent en coordonnées canvas (pas de
/// transform héritée — comme Affinity).
#[derive(Clone, Debug)]
pub struct GroupLayer {
    pub id: Uuid,
    pub name: String,
    pub children: Vec<LayerNode>,
    /// État d'affichage du panneau Calques (replié/déplié)
    pub collapsed: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    /// Masques de couverture du groupe — fusionnés multiplicativement.
    pub masks: Vec<LayerMask>,
}

impl GroupLayer {
    pub fn new(name: impl Into<String>, children: Vec<LayerNode>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            children,
            collapsed: false,
            opacity: 100.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            masks: Vec::new(),
        }
    }
}

/// Calque d'ajustement : sa chaîne de filtres s'applique à la COMPOSITE
/// accumulée en dessous de lui, dans la limite de son groupe.
#[derive(Clone, Debug)]
pub struct AdjustmentLayer {
    pub id: Uuid,
    pub name: String,
    pub filters: Vec<FilterNode>,
    /// Pondération de l'ajustement (0 = invisible, 100 = plein)
    pub opacity: f32,
    pub visible: bool,
}

impl AdjustmentLayer {
    pub fn new(name: impl Into<String>, filters: Vec<FilterNode>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            filters,
            opacity: 100.0,
            visible: true,
        }
    }
}

/// Nœud de l'arbre : calque pixels, groupe ou ajustement.
#[derive(Clone, Debug)]
pub enum LayerNode {
    Pixel(PixelLayer),
    Group(GroupLayer),
    Adjustment(AdjustmentLayer),
}

impl LayerNode {
    pub fn id(&self) -> Uuid {
        match self {
            LayerNode::Pixel(l) => l.id,
            LayerNode::Group(g) => g.id,
            LayerNode::Adjustment(a) => a.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            LayerNode::Pixel(l) => &l.name,
            LayerNode::Group(g) => &g.name,
            LayerNode::Adjustment(a) => &a.name,
        }
    }

    pub fn set_name(&mut self, name: String) {
        match self {
            LayerNode::Pixel(l) => l.name = name,
            LayerNode::Group(g) => g.name = name,
            LayerNode::Adjustment(a) => a.name = name,
        }
    }

    pub fn visible(&self) -> bool {
        match self {
            LayerNode::Pixel(l) => l.visible,
            LayerNode::Group(g) => g.visible,
            LayerNode::Adjustment(a) => a.visible,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        match self {
            LayerNode::Pixel(l) => l.visible = visible,
            LayerNode::Group(g) => g.visible = visible,
            LayerNode::Adjustment(a) => a.visible = visible,
        }
    }

    pub fn opacity(&self) -> f32 {
        match self {
            LayerNode::Pixel(l) => l.opacity,
            LayerNode::Group(g) => g.opacity,
            LayerNode::Adjustment(a) => a.opacity,
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        let clamped = opacity.clamp(0.0, 100.0);
        match self {
            LayerNode::Pixel(l) => l.opacity = clamped,
            LayerNode::Group(g) => g.opacity = clamped,
            LayerNode::Adjustment(a) => a.opacity = clamped,
        }
    }

    /// Mode de fusion — absent des calques d'ajustement.
    pub fn blend_mode(&self) -> Option<BlendMode> {
        match self {
            LayerNode::Pixel(l) => Some(l.blend_mode),
            LayerNode::Group(g) => Some(g.blend_mode),
            LayerNode::Adjustment(_) => None,
        }
    }

    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        match self {
            LayerNode::Pixel(l) => l.blend_mode = mode,
            LayerNode::Group(g) => g.blend_mode = mode,
            LayerNode::Adjustment(_) => {}
        }
    }

    /// Chaîne de filtres d'un calque D'AJUSTEMENT (les pixels portent des
    /// sous-calques [`FilterLayer`] — voir `PixelLayer::filter_layers`).
    pub fn filters(&self) -> Option<&Vec<FilterNode>> {
        match self {
            LayerNode::Pixel(_) => None,
            LayerNode::Adjustment(a) => Some(&a.filters),
            LayerNode::Group(_) => None,
        }
    }

    pub fn filters_mut(&mut self) -> Option<&mut Vec<FilterNode>> {
        match self {
            LayerNode::Pixel(_) => None,
            LayerNode::Adjustment(a) => Some(&mut a.filters),
            LayerNode::Group(_) => None,
        }
    }

    /// Tous les masques de couverture d'un calque pixels ou groupe.
    pub fn masks(&self) -> &[LayerMask] {
        match self {
            LayerNode::Pixel(l) => &l.masks,
            LayerNode::Group(g) => &g.masks,
            LayerNode::Adjustment(_) => &[],
        }
    }

    pub fn masks_mut(&mut self) -> Option<&mut Vec<LayerMask>> {
        match self {
            LayerNode::Pixel(l) => Some(&mut l.masks),
            LayerNode::Group(g) => Some(&mut g.masks),
            LayerNode::Adjustment(_) => None,
        }
    }

    /// Masque identifié par son id, s'il existe.
    pub fn mask(&self, id: Uuid) -> Option<&LayerMask> {
        self.masks().iter().find(|m| m.id == id)
    }

    pub fn mask_mut(&mut self, id: Uuid) -> Option<&mut LayerMask> {
        self.masks_mut()?.iter_mut().find(|m| m.id == id)
    }

    /// Décale la transform du calque de `(dx, dy)` pixels document
    /// (outil déplacement). Seuls les calques pixels portent une
    /// transform : groupes et ajustements retournent `false` (no-op).
    /// Retourne `true` si le calque a été déplacé.
    pub fn translate_by(&mut self, dx: f32, dy: f32) -> bool {
        match self {
            LayerNode::Pixel(l) => {
                l.transform.offset_x += dx;
                l.transform.offset_y += dy;
                l.touch();
                true
            }
            LayerNode::Group(_) | LayerNode::Adjustment(_) => false,
        }
    }

    /// Réassigne des identifiants frais (duplication) et invalide l'apparence.
    /// Les masques sont inclus : ils sont adressés par id (miniatures UI).
    pub(crate) fn regenerate_ids(&mut self) {
        match self {
            LayerNode::Pixel(l) => {
                l.id = Uuid::new_v4();
                for f in &mut l.filter_layers {
                    f.id = Uuid::new_v4();
                    for m in &mut f.masks {
                        m.id = Uuid::new_v4();
                    }
                }
                for m in &mut l.masks {
                    m.id = Uuid::new_v4();
                }
                l.touch();
            }
            LayerNode::Group(g) => {
                g.id = Uuid::new_v4();
                for m in &mut g.masks {
                    m.id = Uuid::new_v4();
                }
                for c in &mut g.children {
                    c.regenerate_ids();
                }
            }
            LayerNode::Adjustment(a) => {
                a.id = Uuid::new_v4();
                for f in &mut a.filters {
                    f.id = Uuid::new_v4();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Document : racine de l'arbre + cache d'apparence
// ---------------------------------------------------------------------------

/// Apparence dérivée d'un calque pixels (source × filtres actifs).
#[derive(Clone)]
pub struct Appearance {
    /// Image pleine résolution (export, transforms)
    pub image: Arc<DynamicImage>,
    /// Buffer d'affichage interactif (downscalé au-delà de 2048 px)
    pub preview: RgbaBuf,
    /// Miniature 48×32 pour le panneau Calques
    pub thumb: RgbaBuf,
}
