// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Composition program of one output tile (docs/decisions/0012 and 0021):
//! the layer tree flattened into operations that the composite shader
//! interprets with a stack of registers.
//!
//! - A pixel layer is one `Layer` operation (`Clipped` when clipped).
//! - A group pushes a register, composes its children, then pops: an
//!   isolated group blends its result with its mode and opacity, a
//!   pass-through group mixes it with the saved backdrop.
//! - A clipping set (a base and the clipped layers above it) is an isolated
//!   group: the base at full opacity, the clipped layers in source-atop,
//!   then the whole blended with the opacity and mode of the base.
//!
//! Nodes that contribute nothing to the tile (hidden, no pixels there,
//! empty groups) produce no operation.

use cygnus_photo_core::pixels::TileCoord;
use cygnus_photo_core::{BlendMode, Depth, Document, Isolation, MAX_NESTING, NodeId, NodeKind};

use crate::renderer::RenderError;
use crate::sources::SourceKey;

/// Registers of the composite shader (`STACK_DEPTH` in WGSL, generated):
/// the nesting limit of the model.
pub(crate) const STACK_DEPTH: usize = MAX_NESTING;

/// Operation codes, as in the composite shader (`OP_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    /// Blend a layer tile over the accumulator.
    Layer = 0,
    /// Compose a layer tile in source-atop (clipped layer).
    Clipped = 1,
    /// Save the accumulator and start a transparent one.
    PushIsolated = 2,
    /// Save the accumulator and keep composing on it.
    PushPassThrough = 3,
    /// Blend the group result over the saved accumulator.
    PopIsolated = 4,
    /// Mix the saved accumulator and the group result.
    PopPassThrough = 5,
    /// Compose the group result in source-atop over the saved accumulator.
    PopClipped = 6,
}

/// Mask of an operation, as the shader evaluates it: coverage `m` read in
/// the mask tile, or `value` where the mask has no tile; inverted if asked;
/// then `1 − density × (1 − m)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MaskRef {
    pub(crate) tile: Option<SourceKey>,
    pub(crate) value: f32,
    pub(crate) density: f32,
    pub(crate) inverted: bool,
}

impl MaskRef {
    /// No mask: full coverage.
    pub(crate) const NONE: Self = Self {
        tile: None,
        value: 1.0,
        density: 0.0,
        inverted: false,
    };
}

/// One operation of a tile program.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Op {
    pub(crate) code: Code,
    /// Layer tile read by `Layer` and `Clipped`.
    pub(crate) source: Option<SourceKey>,
    pub(crate) mode: BlendMode,
    pub(crate) opacity: f32,
    pub(crate) mask: MaskRef,
}

impl Op {
    fn marker(code: Code) -> Self {
        Self {
            code,
            source: None,
            mode: BlendMode::Normal,
            opacity: 1.0,
            mask: MaskRef::NONE,
        }
    }

    fn pop(code: Code, mode: BlendMode, opacity: f32, mask: MaskRef) -> Self {
        Self {
            code,
            source: None,
            mode,
            opacity,
            mask,
        }
    }
}

/// How a node takes part in its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// On its own.
    Normal,
    /// Base of a clipping set: composed at full opacity in Normal mode,
    /// its own opacity and mode applying to the whole set.
    Base,
    /// Clipped to the base below it.
    Clipped,
}

/// Program of the output tile at `coord`.
pub(crate) fn tile_program(document: &Document, coord: TileCoord) -> Result<Vec<Op>, RenderError> {
    let mut ops = Vec::new();
    children(document, document.root, coord, &mut ops)?;
    let mut depth = 0usize;
    for op in &ops {
        match op.code {
            Code::PushIsolated | Code::PushPassThrough => {
                depth += 1;
                if depth > STACK_DEPTH {
                    return Err(RenderError::Unsupported(
                        "groups and clipping sets nested deeper than 8 levels",
                    ));
                }
            }
            Code::PopIsolated | Code::PopPassThrough | Code::PopClipped => depth -= 1,
            Code::Layer | Code::Clipped => {}
        }
    }
    Ok(ops)
}

/// Layer tiles and mask tiles a program reads.
pub(crate) fn sources(ops: &[Op]) -> (Vec<SourceKey>, Vec<SourceKey>) {
    let mut layers: Vec<SourceKey> = ops.iter().filter_map(|op| op.source).collect();
    let mut masks: Vec<SourceKey> = ops.iter().filter_map(|op| op.mask.tile).collect();
    for keys in [&mut layers, &mut masks] {
        keys.sort();
        keys.dedup();
    }
    (layers, masks)
}

fn children(
    document: &Document,
    parent: NodeId,
    coord: TileCoord,
    out: &mut Vec<Op>,
) -> Result<(), RenderError> {
    let ids: Vec<NodeId> = document
        .children(parent)
        .iter()
        .copied()
        .filter(|id| document.node(*id).is_some_and(|node| node.kind.is_layer()))
        .collect();
    let clipped = |id: &NodeId| document.node(*id).is_some_and(|node| node.clip);
    let mut i = 0;
    while i < ids.len() {
        // The first node of a group cannot be clipped: it acts as a base.
        let base = ids[i];
        let mut end = i + 1;
        while end < ids.len() && clipped(&ids[end]) {
            end += 1;
        }
        if end == i + 1 {
            node(document, base, coord, Role::Normal, out)?;
        } else {
            clipping_set(document, base, &ids[i + 1..end], coord, out)?;
        }
        i = end;
    }
    Ok(())
}

fn clipping_set(
    document: &Document,
    base: NodeId,
    clipped: &[NodeId],
    coord: TileCoord,
    out: &mut Vec<Op>,
) -> Result<(), RenderError> {
    let mut base_ops = Vec::new();
    node(document, base, coord, Role::Base, &mut base_ops)?;
    if base_ops.is_empty() {
        // Nothing to clip to: the clipped layers show nowhere.
        return Ok(());
    }
    let mut clipped_ops = Vec::new();
    for id in clipped {
        node(document, *id, coord, Role::Clipped, &mut clipped_ops)?;
    }
    if clipped_ops.is_empty() {
        return node(document, base, coord, Role::Normal, out);
    }
    let Some(base_node) = document.node(base) else {
        return Ok(());
    };
    out.push(Op::marker(Code::PushIsolated));
    out.extend(base_ops);
    out.extend(clipped_ops);
    out.push(Op::pop(
        Code::PopIsolated,
        base_node.blend,
        base_node.opacity,
        MaskRef::NONE,
    ));
    Ok(())
}

fn node(
    document: &Document,
    id: NodeId,
    coord: TileCoord,
    role: Role,
    out: &mut Vec<Op>,
) -> Result<(), RenderError> {
    let Some(node) = document.node(id) else {
        return Ok(());
    };
    if !node.visible {
        return Ok(());
    }
    if !node.blend.is_supported() {
        return Err(RenderError::UnsupportedBlendMode(node.blend));
    }
    let mask = mask(document, node.mask, coord);
    let (mode, opacity) = match role {
        Role::Base => (BlendMode::Normal, 1.0),
        Role::Normal | Role::Clipped => (node.blend, node.opacity),
    };
    match &node.kind {
        NodeKind::Pixel { .. } => {
            if document.pixels.tile(id, coord).is_some() {
                out.push(Op {
                    code: if role == Role::Clipped {
                        Code::Clipped
                    } else {
                        Code::Layer
                    },
                    source: Some((id, coord)),
                    mode,
                    opacity,
                    mask,
                });
            }
        }
        NodeKind::Group { isolation, .. } => {
            let mut inner = Vec::new();
            children(document, id, coord, &mut inner)?;
            if inner.is_empty() {
                return Ok(());
            }
            let (push, pop) = match (role, isolation) {
                (Role::Normal, Isolation::PassThrough) => {
                    (Code::PushPassThrough, Code::PopPassThrough)
                }
                (Role::Normal | Role::Base, _) => (Code::PushIsolated, Code::PopIsolated),
                (Role::Clipped, _) => (Code::PushIsolated, Code::PopClipped),
            };
            out.push(Op::marker(push));
            out.extend(inner);
            out.push(Op::pop(pop, mode, opacity, mask));
        }
        NodeKind::Adjustment { .. } => {
            return Err(RenderError::Unsupported("rendering an adjustment layer"));
        }
        NodeKind::Filter { .. } => {
            return Err(RenderError::Unsupported("rendering a filter layer"));
        }
        NodeKind::Embedded { .. } => {
            return Err(RenderError::Unsupported("rendering an embedded object"));
        }
        NodeKind::Mask { .. } => {}
    }
    Ok(())
}

fn mask(document: &Document, mask: Option<NodeId>, coord: TileCoord) -> MaskRef {
    let Some(id) = mask else {
        return MaskRef::NONE;
    };
    let Some(NodeKind::Mask {
        density,
        inverted,
        enabled,
        empty,
        ..
    }) = document.node(id).map(|node| &node.kind)
    else {
        return MaskRef::NONE;
    };
    if !enabled {
        return MaskRef::NONE;
    }
    let max = match document.depth {
        Depth::U8 => 255.0,
        Depth::U16 => 65535.0,
    };
    MaskRef {
        tile: document.pixels.tile(id, coord).map(|_| (id, coord)),
        value: f32::from(*empty) / max,
        density: *density,
        inverted: *inverted,
    }
}
