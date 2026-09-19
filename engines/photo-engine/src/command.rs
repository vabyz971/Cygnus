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

//! Commandes légères d'historique (Command Pattern) pour les micro-éditions.
//!
//! Une [`Command`] capture une transition old → new sur UN nœud : elle se
//! stocke pour quelques octets (contre un clonage complet de l'arbre pour un
//! Snapshot) et sait se inverser ([`Command::inverse`]) et décrire son
//! impact rendu ([`Command::render_event`]).
//!
//! Les types de valeurs réutilisent le modèle existant — AUCUNE duplication :
//! - [`crate::document::Transform2D`] et [`crate::document::BlendMode`]
//! - `datatypes::ParamValue`

use datatypes::ParamValue;
use uuid::Uuid;

use crate::document::{BlendMode, Transform2D};

/// Commande légère : modification d'un paramètre d'un nœud existant.
///
/// Conventions :
/// - `old`/`new` portent les valeurs AVANT/APRÈS ;
/// - [`crate::Document::apply_command`] applique `new` et retourne
///   l'inverse (old/new échangés) prêt à empiler dans l'historique ;
/// - si le nœud cible a disparu (undo inter-langue avec suppression),
///   la commande est retournée telle quelle (no-op sûr).
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    SetOpacity {
        layer_id: Uuid,
        old: f32,
        new: f32,
    },
    SetTransform {
        layer_id: Uuid,
        old: Transform2D,
        new: Transform2D,
    },
    SetBlendMode {
        node_id: Uuid,
        old: BlendMode,
        new: BlendMode,
    },
    SetVisibility {
        node_id: Uuid,
        old: bool,
        new: bool,
    },
    SetFilterParam {
        layer_id: Uuid,
        filter_id: Uuid,
        param_name: String,
        old: ParamValue,
        new: ParamValue,
    },
    RenameLayer {
        node_id: Uuid,
        old: String,
        new: String,
    },
    SetMaskEnabled {
        node_id: Uuid,
        mask_id: Uuid,
        old: bool,
        new: bool,
    },
    SetMaskInverted {
        node_id: Uuid,
        mask_id: Uuid,
        old: bool,
        new: bool,
    },
}

impl Command {
    /// Identifiant du nœud ciblé (pour invalidation UI ciblée).
    #[must_use]
    pub fn target(&self) -> Uuid {
        match self {
            Command::SetOpacity { layer_id, .. }
            | Command::SetTransform { layer_id, .. }
            | Command::SetFilterParam { layer_id, .. } => *layer_id,
            Command::SetBlendMode { node_id, .. }
            | Command::SetVisibility { node_id, .. }
            | Command::RenameLayer { node_id, .. }
            | Command::SetMaskEnabled { node_id, .. }
            | Command::SetMaskInverted { node_id, .. } => *node_id,
        }
    }

    /// Inverse exact : appliquer la commande puis son inverse restaure
    /// l'état initial (et réciproquement).
    #[must_use]
    pub fn inverse(&self) -> Command {
        match self {
            Command::SetOpacity { layer_id, old, new } => Command::SetOpacity {
                layer_id: *layer_id,
                old: *new,
                new: *old,
            },
            Command::SetTransform { layer_id, old, new } => Command::SetTransform {
                layer_id: *layer_id,
                old: *new,
                new: *old,
            },
            Command::SetBlendMode { node_id, old, new } => Command::SetBlendMode {
                node_id: *node_id,
                old: *new,
                new: *old,
            },
            Command::SetVisibility { node_id, old, new } => Command::SetVisibility {
                node_id: *node_id,
                old: *new,
                new: *old,
            },
            Command::SetFilterParam {
                layer_id,
                filter_id,
                param_name,
                old,
                new,
            } => Command::SetFilterParam {
                layer_id: *layer_id,
                filter_id: *filter_id,
                param_name: param_name.clone(),
                old: new.clone(),
                new: old.clone(),
            },
            Command::RenameLayer { node_id, old, new } => Command::RenameLayer {
                node_id: *node_id,
                old: new.clone(),
                new: old.clone(),
            },
            Command::SetMaskEnabled {
                node_id,
                mask_id,
                old,
                new,
            } => Command::SetMaskEnabled {
                node_id: *node_id,
                mask_id: *mask_id,
                old: *new,
                new: *old,
            },
            Command::SetMaskInverted {
                node_id,
                mask_id,
                old,
                new,
            } => Command::SetMaskInverted {
                node_id: *node_id,
                mask_id: *mask_id,
                old: *new,
                new: *old,
            },
        }
    }

    /// Fusionne un geste continu : garde l'« old » du DÉBUT du geste et
    /// met à jour uniquement le « new ». Retourne false si les deux
    /// commandes ne décrivent pas la même édition (même variante ET même
    /// cible) — l'appelant empilera alors deux entrées distinctes.
    pub fn merge_forward(&mut self, next: &Command) -> bool {
        match (self, next) {
            (Command::SetOpacity { new, .. }, Command::SetOpacity { new: n2, .. }) => {
                *new = *n2;
                true
            }
            (Command::SetTransform { new, .. }, Command::SetTransform { new: n2, .. }) => {
                *new = *n2;
                true
            }
            (
                Command::SetBlendMode { node_id, new, .. },
                Command::SetBlendMode {
                    node_id: id2,
                    new: n2,
                    ..
                },
            ) if node_id == id2 => {
                *new = *n2;
                true
            }
            (
                Command::SetVisibility { node_id, new, .. },
                Command::SetVisibility {
                    node_id: id2,
                    new: n2,
                    ..
                },
            ) if node_id == id2 => {
                *new = *n2;
                true
            }
            (
                Command::SetFilterParam {
                    layer_id,
                    filter_id,
                    new,
                    ..
                },
                Command::SetFilterParam {
                    layer_id: lid2,
                    filter_id: fid2,
                    new: n2,
                    ..
                },
            ) if layer_id == lid2 && filter_id == fid2 => {
                *new = n2.clone();
                true
            }
            (
                Command::RenameLayer { node_id, new, .. },
                Command::RenameLayer {
                    node_id: id2,
                    new: n2,
                    ..
                },
            ) if node_id == id2 => {
                *new = n2.clone();
                true
            }
            _ => false,
        }
    }

    /// La commande modifie-t-elle la COMPOSITE globale (et non seulement
    /// l'apparence isolée du calque dessiné indépendamment) ?
    ///
    /// - opacité/transform/rename : appliqués au draw GPU — aucun recomposite
    /// - blend mode / visibilité : changent QUI fusionne avec QUI → fallback
    /// - paramètre de filtre sur calque pixels : l'apparence se recalcule
    ///   seule (cache de versions) — pas de recomposite global
    #[must_use]
    pub fn affects_composite(&self) -> bool {
        matches!(
            self,
            Command::SetBlendMode { .. }
                | Command::SetVisibility { .. }
                | Command::SetMaskEnabled { .. }
                | Command::SetMaskInverted { .. }
        )
    }
}

/// Notification au moteur de rendu : quoi invalider après une édition.
/// Le cache de textures UI (PreviewCache) affine encore ce signal par
/// comparaison d'adresses d'Arc — les calques inchangés gardent leurs
/// textures GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderEvent {
    /// Tout re-rendre (structure, undo snapshot, ouverture projet…)
    FullInvalidation,
    /// Un seul nœud a changé
    NodeInvalidated(Uuid),
}

impl Command {
    /// Événement rendu induit par cette commande.
    #[must_use]
    pub fn render_event(&self) -> RenderEvent {
        if self.affects_composite() {
            // Le blending global dépend de ce nœud : recomposite complet
            RenderEvent::FullInvalidation
        } else {
            RenderEvent::NodeInvalidated(self.target())
        }
    }
}

/// Révision monotone du rendu : incrémentée chaque fois qu'un nouveau
/// composite est produit. L'UI l'attache à la texture affichée et
/// ignore tout rendu dont la révision est obsolète (réponse en retard
/// après coalescence d'un geste continu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderRevision(pub u64);

impl RenderRevision {
    /// Avance d'un cran et retourne la nouvelle révision.
    pub fn bump(&mut self) -> Self {
        self.0 = self.0.wrapping_add(1);
        *self
    }

    /// Vrai si `self` est strictement plus récente que `other`.
    #[must_use]
    pub fn is_newer_than(self, other: Self) -> bool {
        self > other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(x: f32) -> Transform2D {
        Transform2D {
            offset_x: x,
            ..Transform2D::default()
        }
    }

    #[test]
    fn inverse_echange_old_et_new() {
        let cmd = Command::SetOpacity {
            layer_id: Uuid::nil(),
            old: 20.0,
            new: 80.0,
        };
        assert_eq!(
            cmd.inverse(),
            Command::SetOpacity {
                layer_id: Uuid::nil(),
                old: 80.0,
                new: 20.0,
            }
        );
        // Double inverse = identité
        assert_eq!(cmd.inverse().inverse(), cmd);
    }

    #[test]
    fn merge_garde_le_vieil_et_prend_le_nouveau_dernier() {
        let mut base = Command::SetOpacity {
            layer_id: Uuid::nil(),
            old: 50.0,
            new: 60.0,
        };
        assert!(base.merge_forward(&Command::SetOpacity {
            layer_id: Uuid::nil(),
            old: 60.0,
            new: 70.0,
        }));
        assert!(base.merge_forward(&Command::SetOpacity {
            layer_id: Uuid::nil(),
            old: 70.0,
            new: 80.0,
        }));
        // Geste 50→60→70→80 coalescé : old=50 (début), new=80 (fin)
        assert_eq!(
            base,
            Command::SetOpacity {
                layer_id: Uuid::nil(),
                old: 50.0,
                new: 80.0,
            }
        );
    }

    #[test]
    fn merge_refuse_des_cibles_differentes() {
        let mut base = Command::SetFilterParam {
            layer_id: Uuid::nil(),
            filter_id: Uuid::nil(),
            param_name: "brightness".into(),
            old: ParamValue::Float(0.0),
            new: ParamValue::Float(1.0),
        };
        let autre_filtre = Command::SetFilterParam {
            layer_id: Uuid::nil(),
            filter_id: uuid::Uuid::new_v4(),
            param_name: "brightness".into(),
            old: ParamValue::Float(1.0),
            new: ParamValue::Float(2.0),
        };
        assert!(!base.merge_forward(&autre_filtre));
    }

    #[test]
    fn evenement_rendu_par_type() {
        let id = uuid::Uuid::new_v4();
        let opacity = Command::SetOpacity {
            layer_id: id,
            old: 1.0,
            new: 2.0,
        };
        assert_eq!(opacity.render_event(), RenderEvent::NodeInvalidated(id));

        let blend = Command::SetBlendMode {
            node_id: id,
            old: BlendMode::Normal,
            new: BlendMode::Multiply,
        };
        assert_eq!(blend.render_event(), RenderEvent::FullInvalidation);

        let transform = Command::SetTransform {
            layer_id: id,
            old: t(0.0),
            new: t(5.0),
        };
        assert_eq!(transform.render_event(), RenderEvent::NodeInvalidated(id));
    }

    #[test]
    fn revision_bump_est_monotone() {
        let mut revision = RenderRevision::default();
        assert_eq!(revision, RenderRevision(0));
        assert_eq!(revision.bump(), RenderRevision(1));
        assert_eq!(revision.bump(), RenderRevision(2));
        assert!(RenderRevision(2).is_newer_than(RenderRevision(1)));
        assert!(!RenderRevision(1).is_newer_than(RenderRevision(1)));
        assert!(!RenderRevision(1).is_newer_than(RenderRevision(2)));
    }
}
