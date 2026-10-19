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

//! Lightweight history commands (Command Pattern) for micro-edits.
//!
//! A [`Command`] captures an old → new transition on ONE node: it is
//! stored for a few bytes (vs a full tree clone for a Snapshot) and
//! knows how to invert itself ([`Command::inverse`]) and describe its
//! render impact ([`Command::render_event`]).
//!
//! Value types reuse the existing model — NO duplication:
//! - [`crate::document::Transform2D`] et [`crate::document::BlendMode`]
//! - `datatypes::ParamValue`

use datatypes::ParamValue;
use uuid::Uuid;

use crate::document::{BlendMode, Transform2D};

/// Lightweight command: edits one parameter of an existing node.
///
/// Conventions:
/// - `old`/`new` carry the BEFORE/AFTER values;
/// - [`crate::Document::apply_command`] applies `new` and returns the
///   inverse (old/new swapped) ready to push into history;
/// - if the target node is gone (undo interleaved with deletion),
///   the command is returned as is (safe no-op).
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
    /// Targeted node's id (for targeted UI invalidation).
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

    /// Exact inverse: applying the command then its inverse restores the
    /// initial state (and vice versa).
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

    /// Merges a continuous gesture: keeps the gesture START's "old" and
    /// only updates the "new". Returns false if both commands do not
    /// describe the same edit (same variant AND same target) — the caller
    /// then pushes two distinct entries.
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

    /// Does the command modify the GLOBAL composite (and not just the
    /// isolated appearance of the independently drawn layer)?
    ///
    /// - opacity/transform/rename: applied at GPU draw — no recomposite
    /// - blend mode / visibility: change WHO blends with WHOM → fallback
    /// - filter parameter on a pixel layer: appearance recomputes alone
    ///   (version cache) — no global recomposite
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

/// Render engine notification: what to invalidate after an edit. The UI
/// texture cache (PreviewCache) further refines this signal via Arc
/// address comparison — unchanged layers keep their GPU
/// textures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderEvent {
    /// Re-render everything (structure, snapshot undo, project open…)
    FullInvalidation,
    /// A single node changed
    NodeInvalidated(Uuid),
}

impl Command {
    /// Render event induced by this command.
    #[must_use]
    pub fn render_event(&self) -> RenderEvent {
        if self.affects_composite() {
            // Global blending depends on this node: full recomposite
            RenderEvent::FullInvalidation
        } else {
            RenderEvent::NodeInvalidated(self.target())
        }
    }
}

/// Monotonic render revision: incremented each time a new composite is
/// produced. The UI attaches it to the displayed texture and ignores any
/// render whose revision is stale (late response after coalescing a
/// continuous gesture).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderRevision(pub u64);

impl RenderRevision {
    /// Advances one step and returns the new revision.
    pub fn bump(&mut self) -> Self {
        self.0 = self.0.wrapping_add(1);
        *self
    }

    /// True if `self` is strictly newer than `other`.
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
        // Double inverse = identity
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
        // Coalesced 50→60→70→80 gesture: old=50 (start), new=80 (end)
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
