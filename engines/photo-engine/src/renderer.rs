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

//! Rendu des apparences par calque avec CACHE CIBLÉ.
//!
//! [`Renderer`] est l'orchestrateur unique du calcul d'apparence
//! (source × live filters). Pour chaque calque :
//!
//! 1. clé de validité = (identité du calque, SIGNATURE ordonnée de sa
//!    chaîne de filtres, identité physique de la source) ;
//! 2. HIT → l'apparence mise en cache est retournée telle quelle :
//!    ZÉRO recalcul, zéro dispatch compute, zéro allocation pixel ;
//! 3. MISS → la chaîne est exécutée via [`crate::filters::render_chain`],
//!    qui route chaque effet vers les COMPUTE SHADERS de [`crate::gpu`]
//!    quand un adaptateur est disponible (fallback CPU rayon sinon),
//!    puis le résultat (image pleine résolution + preview + miniature)
//!    est inséré dans le cache.
//!
//! L'identité de la source est validée par comparaison de pointeurs Arc
//! (avec keep-alive dans l'entrée pour empêcher toute réutilisation
//! d'adresse — même défense ABA que le cache de textures UI).
//!
//! Les compteurs [`Renderer::hits`]/[`Renderer::misses`] rendent le
//! comportement observable : éditer le calque N ne doit provoquer qu'un
//! MISS sur N, jamais sur ses voisins — c'est la promesse « cache GPU
//! ciblé » du LayerTree.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgba};
use uuid::Uuid;

use crate::document::{Appearance, Document, FilterLayer, LayerMask, PixelLayer};

/// Entrée de cache pré-calculée HORS thread UI, transférable vers le
/// document vivant.
///
/// Cas d'usage : l'activation/désactivation d'un filtre (shader) ou d'un
/// masque change la signature → `appearance_hit` MISS → `appearance()`
/// exécuterait `render_chain` + bake + preview/thumb EN PLEINE RÉSOLUTION
/// sur le thread UI (freeze de plusieurs centaines de ms sur les grandes
/// images). Au lieu de cela, l'app clone le document, applique le toggle
/// sur le clone en `spawn_blocking`, exporte l'entrée chaude et l'insère
/// dans le document vivant à la réception : `PreviewCache::sync()` HIT
/// alors sans jamais toucher au pool depuis l'UI.
///
/// Tous les champs sont des `Arc` (zéro copie pixel) : le transfert
/// inter-threads est bon marché. `Send` requis pour `Task::perform`,
/// `Debug` manuel (jamais de dump pixels dans les logs).
#[derive(Clone)]
pub struct WarmedAppearance {
    /// Signature ordonnée de la chaîne de filtres APRES toggle.
    pub filter_signature: u64,
    /// Source conservée vivante (validation par identité de pointeur).
    pub source: Arc<DynamicImage>,
    /// Image non masquée (chaîne de filtres seule).
    pub unmasked: Arc<DynamicImage>,
    /// Signature de la couverture de masques APRES toggle.
    pub mask_signature: u64,
    /// Couverture combinée des masques actifs (`None` = aucun).
    pub mask_cover: Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
    /// Apparence dérivée complète (image + preview + miniature).
    pub appearance: Appearance,
}

impl std::fmt::Debug for WarmedAppearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarmedAppearance")
            .field("filter_signature", &self.filter_signature)
            .field("mask_signature", &self.mask_signature)
            .field(
                "preview_dims",
                &(
                    self.appearance.preview.width,
                    self.appearance.preview.height,
                ),
            )
            .finish()
    }
}

/// Entrée de cache : apparence dérivée + preuves de validité.
///
/// L'image NON masquée (source × filtres) et la couverture de masques sont
/// cacheées SÉPARÉMENT : peindre un masque ne ré-exécute jamais la chaîne
/// de filtres, elle ne fait que recombiner la couverture avec l'image non
/// masquée déjà chaude. La couverture séparée est aussi la donnée qu'un
/// futur chemin shader échantillonnera directement au draw.
struct CacheEntry {
    /// Signature ordonnée de la chaîne de filtres au moment du calcul.
    filter_signature: u64,
    /// Source conservée vivante : validation par identité de pointeur
    /// (une peinture/remplacement produit un nouvel Arc → miss garanti).
    source: Arc<DynamicImage>,
    /// Image non masquée (chaîne de filtres seule), réutilisée quand seuls
    /// les masques changent.
    unmasked: Arc<DynamicImage>,
    /// Signature de la couverture de masques au moment du calcul.
    mask_signature: u64,
    /// Couverture combinée des masques actifs (`None` = aucun masque actif).
    mask_cover: Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
    appearance: Appearance,
}

/// Cache d'apparences par calque, alimenté par la chaîne de rendu
/// (compute shaders GPU lorsque disponibles, CPU rayon sinon).
#[derive(Default)]
pub struct Renderer {
    entries: HashMap<Uuid, CacheEntry>,
    hits: u64,
    misses: u64,
    /// Buffers `preview` (pleine résolution réduite) régénérés.
    preview_rebuilds: u64,
    /// Miniatures `thumb` régénérées.
    thumb_rebuilds: u64,
}

/// Compteurs d'apparences (observabilité, sans effet sur le rendu).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceStats {
    /// Entrées resservies sans recalcul de chaîne.
    pub hits: u64,
    /// Entrées recalculées (chaîne ré-exécutée).
    pub misses: u64,
    /// Buffers `preview` régénérés (resample inclus).
    pub preview_rebuilds: u64,
    /// Miniatures `thumb` régénérées.
    pub thumb_rebuilds: u64,
}

impl Renderer {
    /// Renderer détaché du backend GPU : les effets resteront sur leur
    /// chemin CPU même si un adaptateur apparaît plus tard. Réservé aux
    /// tests déterministes ; privilégier [`Renderer::new`] en production.
    pub fn without_gpu() -> Self {
        Self::default()
    }

    /// Apparence dérivée du calque, depuis le cache si elle est encore
    /// valide — sinon recalculée (GPU compute si disponible) et insérée.
    pub fn appearance(&mut self, layer: &PixelLayer) -> Appearance {
        // Le chemin MISS (render_chain + preview/thumb) est un calcul image
        // lourd : exécuté sur le pool de rendu DÉDIÉ (jamais global) même
        // quand il est déclenché par la synchronisation de l'UI. À chaud,
        // le surcoût d'install est négligeable.
        crate::render_pool::run_parallel(|| self.appearance_locked(layer))
    }

    /// Corps de [`Renderer::appearance`] — jamais appelé directement.
    ///
    /// L'image non masquée et la couverture sont réutilisées séparément :
    /// une édition de masque ne ré-exécute jamais la chaîne de filtres.
    /// Le pixels rendus restent identiques au chemin baké historique.
    ///
    /// Si l'entrée est encore valide ([`appearance_hit`](Self::appearance_hit)),
    /// l'apparence mémorisée est resservie telle quelle : aucun `bake`,
    /// aucun resample `preview`/`thumb` — même buffers partagés.
    fn appearance_locked(&mut self, layer: &PixelLayer) -> Appearance {
        if let Some(cached) = self.appearance_hit(layer) {
            self.hits += 1;
            return cached;
        }
        let unmasked = self.unmasked_image(layer);
        let mask_signature = mask_signature(&layer.masks);
        let mask_cover = match self.entries.get_mut(&layer.id) {
            Some(entry) if entry.mask_signature == mask_signature => entry.mask_cover.clone(),
            Some(entry) => {
                let cover = Self::fresh_cover(&layer.masks);
                entry.mask_signature = mask_signature;
                entry.mask_cover = cover.clone();
                cover
            }
            // Inatteignable en pratique : `unmasked_image` crée l'entrée.
            None => Self::fresh_cover(&layer.masks),
        };
        let baked = Self::bake(&unmasked, &mask_cover);
        let appearance = Appearance {
            preview: crate::document::preview_buf(&baked),
            thumb: crate::document::thumb_buf(&baked),
            image: baked,
        };
        self.preview_rebuilds += 1;
        self.thumb_rebuilds += 1;
        if let Some(entry) = self.entries.get_mut(&layer.id) {
            entry.appearance = appearance.clone();
        }
        appearance
    }

    /// Image NON masquée par les masques DU CALQUE (source × filtres, les
    /// masques des sous-calques restant bakés dans la chaîne car ils
    /// s'appliquent avant transform/fusion de chaque filtre).
    ///
    /// Peindre un masque de calque ne ré-exécute jamais la chaîne : l'appel
    /// suivant retrouve la même image (même `Arc`) tant que source et
    /// filtres sont inchangés. C'est aussi l'image qu'un futur chemin
    /// shader combinera avec la couverture au draw.
    pub fn unmasked_image(&mut self, layer: &PixelLayer) -> Arc<DynamicImage> {
        let filter_signature = filters_signature(&layer.filter_layers);
        // perf-entry-api: use Entry to avoid double hashing on miss path
        use std::collections::hash_map::Entry;
        match self.entries.entry(layer.id) {
            Entry::Occupied(entry)
                if entry.get().filter_signature == filter_signature
                    && Arc::ptr_eq(&entry.get().source, &layer.source_image) =>
            {
                self.hits += 1;
                Arc::clone(&entry.get().unmasked)
            }
            Entry::Occupied(entry) => {
                self.misses += 1;
                let unmasked =
                    crate::filters::render_chain(&layer.source_image, &layer.filter_layers);
                let slot = entry.into_mut();
                slot.filter_signature = filter_signature;
                slot.source = Arc::clone(&layer.source_image);
                slot.unmasked = Arc::clone(&unmasked);
                unmasked
            }
            Entry::Vacant(entry) => {
                self.misses += 1;
                let unmasked =
                    crate::filters::render_chain(&layer.source_image, &layer.filter_layers);
                entry.insert(CacheEntry {
                    filter_signature,
                    source: Arc::clone(&layer.source_image),
                    unmasked: Arc::clone(&unmasked),
                    // Forcée périmée : la couverture est (re)calculée par
                    // l'appelant (`appearance_locked`) juste après.
                    mask_signature: mask_signature(&layer.masks).wrapping_add(1),
                    mask_cover: None,
                    appearance: Appearance {
                        preview: crate::document::preview_buf(&unmasked),
                        thumb: crate::document::thumb_buf(&unmasked),
                        image: Arc::clone(&unmasked),
                    },
                });
                unmasked
            }
        }
    }

    /// Couverture combinée des masques ACTIFS du calque (`None` = aucun),
    /// cacheable séparément. Donnée pure destinée à être échantillonnée
    /// directement par un shader au draw (aucune dépendance UI ici).
    pub fn mask_coverage(
        &mut self,
        layer: &PixelLayer,
    ) -> Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>> {
        let signature = mask_signature(&layer.masks);
        if let Some(entry) = self.entries.get_mut(&layer.id) {
            if entry.mask_signature == signature {
                return entry.mask_cover.clone();
            }
            let cover = Self::fresh_cover(&layer.masks);
            entry.mask_signature = signature;
            entry.mask_cover = cover.clone();
            return cover;
        }
        Self::fresh_cover(&layer.masks)
    }

    fn fresh_cover(masks: &[LayerMask]) -> Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>> {
        crate::document::compositing::combined_mask_coverage(masks).map(Arc::new)
    }

    /// Combine une image non masquée avec une couverture : pixels identiques
    /// à l'ancien chemin baké (`apply_layer_masks`).
    fn bake(
        unmasked: &Arc<DynamicImage>,
        cover: &Option<Arc<ImageBuffer<Rgba<u8>, Vec<u8>>>>,
    ) -> Arc<DynamicImage> {
        match cover {
            None => Arc::clone(unmasked),
            Some(cover) => {
                crate::document::compositing::apply_coverage(Arc::clone(unmasked), cover)
            }
        }
    }

    /// Lookup cache STRICT — aucune exécution, ni pool de rendu ni
    /// `render_chain`. Retourne l'apparence seulement si l'entrée est
    /// encore VALIDE (même signature de filtres, même source par identité
    /// d'Arc), `None` sinon. Réservé à la synchronisation UI
    /// (`PreviewCache::sync`) : pendant un geste (drag, slider…) où rien ne
    /// change les pixels, chaque message re-valide ainsi tous les calques
    /// sans jamais toucher au pool — le « rechargement de la vue » disparaît.
    ///
    /// Le `None` n'est pas une erreur : l'appelant doit alors basculer sur
    /// [`Self::appearance`] (le seul chemin qui exécute la chaîne).
    pub fn appearance_hit(&mut self, layer: &PixelLayer) -> Option<Appearance> {
        let filter_signature = filters_signature(&layer.filter_layers);
        let mask_signature = mask_signature(&layer.masks);
        match self.entries.get(&layer.id) {
            Some(e)
                if e.filter_signature == filter_signature
                    && e.mask_signature == mask_signature
                    && Arc::ptr_eq(&e.source, &layer.source_image) =>
            {
                Some(e.appearance.clone())
            }
            _ => None,
        }
    }

    /// Exporte l'entrée chaude d'un calque pour transfert inter-threads
    /// (voir [`WarmedAppearance`]). Retourne `None` si le calque n'a pas
    /// d'entrée (jamais calculée) — l'appelant doit d'abord passer par
    /// [`Self::appearance`] sur le clone chauffé.
    pub fn export_warmed(&self, layer_id: Uuid) -> Option<WarmedAppearance> {
        self.entries.get(&layer_id).map(|e| WarmedAppearance {
            filter_signature: e.filter_signature,
            source: Arc::clone(&e.source),
            unmasked: Arc::clone(&e.unmasked),
            mask_signature: e.mask_signature,
            mask_cover: e.mask_cover.clone(),
            appearance: e.appearance.clone(),
        })
    }

    /// Insère une entrée pré-calculée hors thread UI. L'insertion est
    /// structurellement sûre : un futur `appearance_hit` ne HIT que si
    /// signatures + identité de source correspondent — une entrée périmée
    /// (édition concurrente) reste simplement inutilisée, jamais fausse.
    pub fn insert_warmed(&mut self, layer_id: Uuid, warmed: WarmedAppearance) {
        self.entries.insert(
            layer_id,
            CacheEntry {
                filter_signature: warmed.filter_signature,
                source: warmed.source,
                unmasked: warmed.unmasked,
                mask_signature: warmed.mask_signature,
                mask_cover: warmed.mask_cover,
                appearance: warmed.appearance,
            },
        );
    }

    /// Préremplit ce cache avec les entrées ACTUELLEMENT chaudes d'un autre
    /// cache. Sert à réchauffer un `Document` clone (cache froid) à partir du
    /// document VIVANT (cache chaud) avant une tâche de fond : la composite
    /// du clone HIT alors au lieu de re-exécuter la chaîne de rendu complète.
    ///
    /// Aucun recalcul : les `Appearance` sont clonées par `Arc` (zéro copie
    /// pixel). Les entrées qui ne sont plus valides (signature/source
    /// changées) seront simplement revalidées à la demande et écrasées.
    pub fn import_from(&mut self, src: &Renderer) {
        for (id, e) in &src.entries {
            self.entries.insert(
                *id,
                CacheEntry {
                    filter_signature: e.filter_signature,
                    source: Arc::clone(&e.source),
                    unmasked: Arc::clone(&e.unmasked),
                    mask_signature: e.mask_signature,
                    mask_cover: e.mask_cover.clone(),
                    appearance: e.appearance.clone(),
                },
            );
        }
    }

    /// Invalide UNIQUEMENT l'apparence du calque donné (micro-édition
    /// imposée manuellement ; normalement inutile : la signature suffit).
    pub fn invalidate_layer(&mut self, layer_id: Uuid) {
        self.entries.remove(&layer_id);
    }

    /// Invalide tout le cache (opération structurelle, restauration…).
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Élague les entrées des calques qui n'existent plus dans l'arbre.
    /// À appeler après une suppression/groupement/duplication ou un undo
    /// structurel — libère la VRAM/RAM des sous-arbres disparus.
    pub fn sync_tree(&mut self, doc: &Document) {
        let live: HashSet<Uuid> = doc.iter_pixels().into_iter().map(|l| l.id).collect();
        self.entries.retain(|id, _| live.contains(id));
    }

    /// Nombre d'apparences actuellement en cache.
    pub fn cached_len(&self) -> usize {
        self.entries.len()
    }

    /// Apparences servies SANS recalcul depuis la construction (ou le
    /// dernier reset des compteurs) — l'indicateur direct du cache ciblé.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Apparences recalculées.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Buffers `preview` régénérés depuis la construction (ou le dernier
    /// reset) : resample inclus, même à signatures inchangées avant le
    /// fast-path [`appearance_hit`](Self::appearance_hit).
    pub fn preview_rebuilds(&self) -> u64 {
        self.preview_rebuilds
    }

    /// Miniatures `thumb` régénérées.
    pub fn thumb_rebuilds(&self) -> u64 {
        self.thumb_rebuilds
    }

    /// Compteurs groupés (observabilité par fenêtre).
    pub fn stats(&self) -> AppearanceStats {
        AppearanceStats {
            hits: self.hits,
            misses: self.misses,
            preview_rebuilds: self.preview_rebuilds,
            thumb_rebuilds: self.thumb_rebuilds,
        }
    }

    /// Remet les compteurs à zéro (observabilité par fenêtre).
    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.preview_rebuilds = 0;
        self.thumb_rebuilds = 0;
    }
}

/// Signature d'une liste de masques : nombre, id, état actif, inversion et
/// version (la peinture remplace le buffer ET touche la version → MISS
/// garanti). Le nom d'un masque en est EXCLU (renommer ne change pas les
/// pixels). Utilisée pour la couverture cacheable SÉPARÉMENT de l'image.
fn mask_signature(masks: &[LayerMask]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write_usize(masks.len());
    for m in masks {
        h.write(m.id.as_bytes());
        h.write_u8(u8::from(m.enabled));
        h.write_u8(u8::from(m.inverted));
        h.write_u64(m.version);
    }
    h.finish()
}

/// Signature ORDONNÉE d'une chaîne de sous-calques de filtres : deux chaînes
/// ont la même signature ssi mêmes sous-calques (id, type, état actif,
/// opacité, fusion, transform, masques) avec mêmes paramètres dans le même
/// ordre. L'ordre est significatif — c'est une CHAÎNE de traitement, pas
/// un ensemble.
///
/// Les masques des sous-calques EN FONT PARTIE : ils sont bakés dans la
/// chaîne par `composite_filter_layer`, donc toute édition (peinture,
/// toggle, inversion) doit invalider le cache — la version du masque
/// l'assure. (Les masques du CALQUE lui-même sont suivis séparément par
/// [`mask_signature`].)
///
/// Déterministe entre processus (`DefaultHasher::new()` = clés fixes),
/// indépendant de l'itération désordonnée de `HashMap` (clés triées).
#[must_use]
pub fn filters_signature(filters: &[FilterLayer]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let mut h = DefaultHasher::new();
    h.write_usize(filters.len());
    for f in filters {
        h.write(f.id.as_bytes());
        h.write(f.type_id.as_bytes());
        h.write_u8(u8::from(f.enabled));
        h.write_u32(f.opacity.to_bits());
        h.write_u32(f.blend_mode.id());
        for v in [
            f.transform.offset_x,
            f.transform.offset_y,
            f.transform.rotation_deg,
            f.transform.scale_x,
            f.transform.scale_y,
            f.transform.skew_x,
            f.transform.skew_y,
        ] {
            h.write_u32(v.to_bits());
        }
        // Masques bakés dans l'apparence : id + état + version (la peinture
        // remplace le buffer ET touche la version → MISS garanti).
        h.write_usize(f.masks.len());
        for m in &f.masks {
            h.write(m.id.as_bytes());
            h.write_u8(u8::from(m.enabled));
            h.write_u8(u8::from(m.inverted));
            h.write_u64(m.version);
        }
        // Params triés par clé : même contenu => même signature
        let mut keys: Vec<&str> = Vec::with_capacity(f.params.len());
        keys.extend(f.params.keys().map(String::as_str));
        keys.sort_unstable();
        h.write_usize(keys.len());
        for key in keys {
            h.write(key.as_bytes());
            hash_param_value(&mut h, &f.params[key]);
        }
    }
    h.finish()
}

fn hash_param_value(
    h: &mut std::collections::hash_map::DefaultHasher,
    value: &datatypes::ParamValue,
) {
    use std::hash::Hasher;
    // Discriminant + bits bruts : f32 exclus de Hash std (NaN), on hashe
    // leurs bits — deux valeurs égales au sens PartialEq ont les mêmes bits.
    match value {
        datatypes::ParamValue::Float(v) => {
            h.write_u8(0);
            h.write_u32(v.to_bits());
        }
        datatypes::ParamValue::Int(v) => {
            h.write_u8(1);
            h.write_i32(*v);
        }
        datatypes::ParamValue::Bool(v) => {
            h.write_u8(2);
            h.write_u8(u8::from(*v));
        }
        datatypes::ParamValue::Color(c) => {
            h.write_u8(3);
            for channel in c {
                h.write_u32(channel.to_bits());
            }
        }
        datatypes::ParamValue::Text(s) => {
            h.write_u8(4);
            h.write(s.as_bytes());
        }
        datatypes::ParamValue::Enum(s) => {
            h.write_u8(5);
            h.write(s.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datatypes::ParamValue;
    use image::ImageBuffer;
    use image::Rgba;

    fn solid(value: u8) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            2,
            2,
            Rgba([value, value, value, 255]),
        )))
    }

    fn layer_with_filter(brightness: f32) -> PixelLayer {
        let mut l = PixelLayer::new("test", solid(100));
        let mut f = FilterLayer::neutral("brightness_contrast", Default::default());
        f.params
            .insert("brightness".to_string(), ParamValue::Float(brightness));
        l.filter_layers.push(f);
        l
    }

    #[test]
    fn premier_acces_miss_puis_hits_sans_recalcul() {
        let layer = layer_with_filter(10.0);
        let mut r = Renderer::default();

        let a1 = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 0));

        let a2 = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert_eq!(r.cached_len(), 1);
        // Même Arc d'image : aucun recopiage de pixels
        assert!(Arc::ptr_eq(&a1.image, &a2.image));
    }

    #[test]
    fn seconde_lecture_ne_reconstruit_pas_preview_thumb() {
        // Le fast-path appearance_hit ressert l'apparence mémorisée :
        // aucun resample, compteurs de rebuilds inchangés.
        let layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let a1 = r.appearance(&layer);
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (1, 1));
        let a2 = r.appearance(&layer);
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (1, 1));
        assert_eq!((r.misses(), r.hits()), (1, 1));
        // Mêmes buffers partagés, pas de clones de pixels.
        assert!(Arc::ptr_eq(&a1.image, &a2.image));
        assert!(Arc::ptr_eq(&a1.thumb.data, &a2.thumb.data));
        assert!(Arc::ptr_eq(&a1.preview.data, &a2.preview.data));
    }

    #[test]
    fn changement_de_masque_rebake_sans_reexecuter_la_chaine() {
        use crate::document::LayerMask;
        let mut layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let _ = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 0));
        // Ajout d'un masque : chaîne épargnée (unmasked HIT), mais
        // couverture + bake + preview/thumb à refaire une fois.
        layer.masks.push(LayerMask::full(2, 2));
        let _ = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (2, 2));
        // Relecture : tout est stable, aucun rebuild.
        let _ = r.appearance(&layer);
        assert_eq!((r.preview_rebuilds(), r.thumb_rebuilds()), (2, 2));
    }

    #[test]
    fn changer_un_parametre_provoque_un_seul_nouveau_miss() {
        let mut layer = layer_with_filter(10.0);
        let mut r = Renderer::default();
        let _ = r.appearance(&layer);

        // Réglage du slider : même id de filtre, nouvelle valeur
        let fid = layer.filter_layers[0].id;
        layer.filter_layers[0]
            .params
            .insert("brightness".to_string(), ParamValue::Float(20.0));
        let _ = r.appearance(&layer);
        assert_eq!(r.misses(), 2, "signature changée → recalcul");
        assert_eq!(r.hits(), 0);

        // Relecture sans changement : retour au HIT
        let _ = r.appearance(&layer);
        assert_eq!((r.misses(), r.hits()), (2, 1));
        let _ = fid;
    }

    #[test]
    fn remplacer_la_source_invalide_memes_sans_filtres() {
        // Chaîne vide : la signature ne bougera jamais — seule
        // l'identité de la source peut détecter une peinture/un crop.
        let mut layer = PixelLayer::new("peinture", solid(50));
        let mut r = Renderer::default();
        let avant = r.appearance(&layer);
        assert_eq!(r.misses(), 1);

        layer.set_source_image((*solid(90)).clone());
        let apres = r.appearance(&layer);
        assert_eq!(r.misses(), 2, "nouvel Arc source → recalcul garanti");
        assert!(!Arc::ptr_eq(&avant.image, &apres.image));
    }

    #[test]
    fn desactiver_un_filtre_change_la_signature() {
        let mut layer = layer_with_filter(30.0);
        let mut r = Renderer::default();
        let _ = r.appearance(&layer);

        layer.filter_layers[0].enabled = false;
        let _ = r.appearance(&layer);
        assert_eq!(r.misses(), 2);

        // Et le résultat redevient la source pure (filtre court-circuité)
        let out = r.appearance(&layer);
        let rgba = out.image.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        assert_eq!(p[0], 100);
    }

    #[test]
    fn apparence_chauffee_inseree_hit_sans_recalcul() {
        // Simule le toggle async : le worker calcule sur son propre
        // renderer, le vivant insère l'entrée et HIT sans exécuter.
        let mut layer = layer_with_filter(30.0);
        let mut worker = Renderer::default();
        let _ = worker.appearance(&layer);

        // Toggle côté worker (désactivation du filtre).
        layer.filter_layers[0].enabled = false;
        let _ = worker.appearance(&layer);
        let warmed = worker.export_warmed(layer.id).expect("entrée chaude");

        // Vivant : cache froid, insertion puis HIT strict sans MISS.
        let mut live = Renderer::default();
        live.insert_warmed(layer.id, warmed);
        let hit = live.appearance_hit(&layer);
        assert!(hit.is_some(), "l'entrée insérée doit HITER");
        assert_eq!(
            (live.misses(), live.hits()),
            (0, 0),
            "aucune exécution côté vivant"
        );

        // Entrée périmée (édition concurrente) : pas de faux HIT.
        layer.filter_layers[0].enabled = true;
        assert!(live.appearance_hit(&layer).is_none());
    }

    #[test]
    fn toggle_masque_sans_touch_hit_apres_insertion() {
        // Miroir d'`apply_toggle_flag` : worker et vivant basculent le
        // même bit SANS `touch()` → même version → même signature → HIT.
        let mut layer = PixelLayer::new("m", solid(100));
        layer.masks.push(LayerMask::full(2, 2));
        // Vivant = clone structurel (Arcs partagés, version copiée),
        // comme un snapshot document.
        let mut live = layer.clone();
        let mut worker = Renderer::default();
        let _ = worker.appearance(&layer);

        layer.masks[0].enabled = false;
        let _ = worker.appearance(&layer);
        let warmed = worker.export_warmed(layer.id).expect("entrée chaude");

        live.masks[0].enabled = false;
        let mut live_r = Renderer::default();
        live_r.insert_warmed(live.id, warmed);
        assert!(
            live_r.appearance_hit(&live).is_some(),
            "même version des deux côtés → HIT"
        );

        // Contrôle : un `touch()` intercalé (peinture) fait diverger →
        // MISS sain, jamais de faux HIT sur des pixels périmés.
        live.masks[0].touch();
        assert!(live_r.appearance_hit(&live).is_none());
    }

    #[test]
    fn sync_tree_elague_les_calques_supprimes() {
        let mut doc = Document::new(4, 4);
        let l1 = PixelLayer::new("a", solid(1));
        let l2 = PixelLayer::new("b", solid(2));
        let id1 = l1.id;
        let _ = l2.id;
        doc.push_layer(crate::document::LayerNode::Pixel(l1));
        doc.push_layer(crate::document::LayerNode::Pixel(l2));

        let mut r = Renderer::default();
        for l in doc.iter_pixels() {
            let _ = r.appearance(l);
        }
        assert_eq!(r.cached_len(), 2);

        // Suppression du calque 1 puis élagage
        let _removed = doc.remove(id1);
        r.sync_tree(&doc);
        assert_eq!(r.cached_len(), 1, "entrée orpheline supprimée");
    }

    #[test]
    fn deux_calques_sont_cachees_independamment() {
        // Le cœur de la promesse LayerTree : toucher le calque A ne
        // re-exécute JAMAIS le calque B.
        let la = layer_with_filter(5.0);
        let lb = layer_with_filter(7.0);
        let mut r = Renderer::default();

        let _ = r.appearance(&la);
        let _ = r.appearance(&lb);
        assert_eq!((r.misses(), r.hits()), (2, 0));

        let mut la_mod = la.clone();
        if let Some(f) = la_mod.filter_layers.first_mut() {
            f.params
                .insert("brightness".to_string(), ParamValue::Float(6.0));
        }
        let _ = r.appearance(&la_mod);
        let _ = r.appearance(&lb);
        // lb n'a généré AUCUN nouveau miss
        assert_eq!((r.misses(), r.hits()), (3, 1));

        // Signature tests (pure partie)
        let s = filters_signature(&la.filter_layers);
        assert_eq!(s, filters_signature(&la.filter_layers));
    }

    #[test]
    fn ordre_et_etat_actif_comptent_dans_la_signature() {
        let mut a = vec![
            filter_of("blur", 3.0),
            filter_of("brightness_contrast", 12.0),
        ];
        let b = vec![
            filter_of("brightness_contrast", 12.0),
            filter_of("blur", 3.0),
        ];
        assert_ne!(filters_signature(&a), filters_signature(&b));

        a[0].enabled = false;
        let disabled_first = vec![a[0].clone(), a[1].clone()];
        let both_on = vec![
            filter_of("blur", 3.0),
            filter_of("brightness_contrast", 12.0),
        ];
        assert_ne!(
            filters_signature(&disabled_first),
            filters_signature(&both_on)
        );
    }

    fn filter_of(name: &str, brightness: f32) -> FilterLayer {
        let mut f = FilterLayer::neutral(name, Default::default());
        f.params
            .insert("brightness".to_string(), ParamValue::Float(brightness));
        f
    }

    #[test]
    fn opacite_et_fusion_comptent_dans_la_signature() {
        let base = vec![filter_of("brightness_contrast", 12.0)];
        let mut attenuated = base.clone();
        attenuated[0].opacity = 50.0;
        assert_ne!(filters_signature(&base), filters_signature(&attenuated));

        let mut multiplied = base.clone();
        multiplied[0].blend_mode = crate::document::BlendMode::Multiply;
        assert_ne!(filters_signature(&base), filters_signature(&multiplied));
    }

    #[test]
    fn masque_sous_calque_invalide_le_cache() {
        use crate::document::LayerMask;
        let base = vec![filter_of("brightness_contrast", 12.0)];
        // Ajout d'un masque → signature différente
        let mut masked = base.clone();
        masked[0].masks.push(LayerMask::full(2, 2));
        assert_ne!(filters_signature(&base), filters_signature(&masked));

        // Peinture (touch = nouvelle version) → signature différente
        let mut touched = masked.clone();
        touched[0].masks[0].touch();
        assert_ne!(filters_signature(&masked), filters_signature(&touched));
    }

    #[test]
    fn couverture_masque_combinee_doree() {
        use crate::document::LayerMask;
        use crate::document::compositing::combined_mask_coverage;
        use image::ImageBuffer;

        // Aucun masque actif → pas de couverture.
        let mut vide = LayerMask::full(2, 2);
        vide.enabled = false;
        assert!(combined_mask_coverage(&[vide]).is_none());
        assert!(combined_mask_coverage(&[]).is_none());

        // 128 × inversé(64→191) = 128*191/255 = 95 (tronqué).
        let mut a = LayerMask::full(2, 2);
        a.image = Arc::new(ImageBuffer::from_pixel(2, 2, Rgba([128, 0, 0, 255])));
        let mut b = LayerMask::full(2, 2);
        b.image = Arc::new(ImageBuffer::from_pixel(2, 2, Rgba([64, 0, 0, 255])));
        b.inverted = true;
        let cover = combined_mask_coverage(&[a, b]).expect("couverture");
        assert_eq!(cover.dimensions(), (2, 2));
        assert_eq!(cover.get_pixel(0, 0)[0], 95);
        assert_eq!(cover.get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn peinture_masque_ne_reexecute_pas_la_chaine() {
        use crate::document::LayerMask;
        let mut layer = layer_with_filter(10.0);
        layer.masks.push(LayerMask::full(2, 2));
        let mut r = Renderer::default();

        let u1 = r.unmasked_image(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 0));

        // Peinture du masque : seule la couverture change, la chaîne NON.
        layer.masks[0].touch();
        let u2 = r.unmasked_image(&layer);
        assert_eq!((r.misses(), r.hits()), (1, 1));
        assert!(Arc::ptr_eq(&u1, &u2), "chaîne non ré-exécutée");

        // La couverture, elle, est bien recalculée et mise en cache.
        let c1 = r.mask_coverage(&layer).expect("couverture");
        let c2 = r.mask_coverage(&layer).expect("couverture");
        assert!(Arc::ptr_eq(&c1, &c2));

        // L'apparence bakée reste correcte : source 100 + filtre 10,
        // masque plein blanc → alpha 255.
        let a = r.appearance(&layer);
        let rgba = a.image.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0)[3], 255);
    }
}
