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

//! Shared datatypes for `Cygnus`
//! Defines generic node bricks used by Photo, Vector, Video...

use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "n{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u32);

// ---------------------------------------------------------------------------
// Socket types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocketType {
    Image,
    Float,
    Color,
    Vector,
    Bool,
}

impl SocketType {
    #[must_use]
    pub fn color(self) -> [f32; 3] {
        match self {
            SocketType::Image => [0.65, 0.45, 0.95],  // violet
            SocketType::Float => [0.55, 0.55, 0.55],  // gray
            SocketType::Color => [0.95, 0.85, 0.25],  // yellow
            SocketType::Vector => [0.30, 0.60, 0.95], // blue
            SocketType::Bool => [0.85, 0.35, 0.35],   // red
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SocketType::Image => "Image",
            SocketType::Float => "Float",
            SocketType::Color => "Color",
            SocketType::Vector => "Vector",
            SocketType::Bool => "Bool",
        }
    }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub enum DataValue {
    #[default]
    None,
    Float(f32),
    Color([f32; 4]),
    Vector([f32; 3]),
    Bool(bool),
    Image(ImageMeta),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMeta {
    pub width: u32,
    pub height: u32,
    pub path: Option<String>,
}

impl DataValue {
    #[must_use]
    pub fn socket_type(&self) -> Option<SocketType> {
        match self {
            DataValue::Float(_) => Some(SocketType::Float),
            DataValue::Color(_) => Some(SocketType::Color),
            DataValue::Vector(_) => Some(SocketType::Vector),
            DataValue::Bool(_) => Some(SocketType::Bool),
            DataValue::Image(_) => Some(SocketType::Image),
            DataValue::None => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Editable node parameters (Properties inspector)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ParamValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    Color([f32; 4]),
    Text(String),
    Enum(String),
}

impl ParamValue {
    #[must_use]
    pub fn as_float(&self) -> Option<f32> {
        if let Self::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    #[must_use]
    pub fn as_enum(&self) -> Option<&str> {
        if let Self::Enum(v) = self {
            Some(v.as_str())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Socket definition (metadata)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SocketDef {
    pub id: String,
    pub name: String,
    pub socket_type: SocketType,
    pub default: Option<DataValue>,
}

impl SocketDef {
    pub fn new(id: impl Into<String>, name: impl Into<String>, ty: SocketType) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            socket_type: ty,
            default: None,
        }
    }

    #[must_use]
    pub fn with_default(mut self, v: DataValue) -> Self {
        self.default = Some(v);
        self
    }
}

// ---------------------------------------------------------------------------
// Categories (for Add Node library)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeCategory {
    Input,
    Output,
    Color,
    Filter,
    Transform,
    Compositing,
    Utility,
}

impl NodeCategory {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            NodeCategory::Input => "Entrée",
            NodeCategory::Output => "Sortie",
            NodeCategory::Color => "Couleur",
            NodeCategory::Filter => "Filtre",
            NodeCategory::Transform => "Transformation",
            NodeCategory::Compositing => "Compositing",
            NodeCategory::Utility => "Utilitaire",
        }
    }
}

// ---------------------------------------------------------------------------
// Node type definition (registry)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NodeDefinition {
    pub type_id: String,
    pub name: String,
    pub category: NodeCategory,
    pub inputs: Vec<SocketDef>,
    pub outputs: Vec<SocketDef>,
    pub default_params: HashMap<String, ParamValue>,
    /// Node header color
    pub header_color: [f32; 3],
    pub description: String,
}

impl NodeDefinition {
    pub fn new(
        type_id: impl Into<String>,
        name: impl Into<String>,
        category: NodeCategory,
    ) -> Self {
        Self {
            type_id: type_id.into(),
            name: name.into(),
            category,
            inputs: Vec::new(),
            outputs: Vec::new(),
            default_params: HashMap::new(),
            header_color: [0.18, 0.18, 0.20],
            description: String::new(),
        }
    }

    #[must_use]
    pub fn input(mut self, def: SocketDef) -> Self {
        self.inputs.push(def);
        self
    }

    #[must_use]
    pub fn output(mut self, def: SocketDef) -> Self {
        self.outputs.push(def);
        self
    }

    /// Adds a default parameter.
    #[must_use]
    pub fn param(mut self, key: impl Into<String>, value: ParamValue) -> Self {
        self.default_params.insert(key.into(), value);
        self
    }

    #[must_use]
    pub fn header_color(mut self, rgb: [f32; 3]) -> Self {
        self.header_color = rgb;
        self
    }

    /// Sets the node description.
    #[must_use]
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Shared UI geometry helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    #[must_use]
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

// ---------------------------------------------------------------------------
// Shared 2D geometry: floating-point rectangle (vector shapes, layout
// frames, text lines). Integer pixel rects live in `tiles` (exact tiling);
// this one is for continuous design-space geometry.
// ---------------------------------------------------------------------------

/// 2D rectangle in continuous space (`x0 <= x1`, `y0 <= y1` expected;
/// empty iff `x1 <= x0 || y1 <= y0`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

impl Rect {
    /// Empty rectangle (no area).
    pub const EMPTY: Self = Self {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    };

    /// New rectangle from bounds.
    #[must_use]
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// New rectangle from origin + dimensions.
    #[must_use]
    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        }
    }

    /// True if it has no area.
    #[must_use]
    pub fn is_empty(self) -> bool {
        !(self.x1 > self.x0 && self.y1 > self.y0)
    }

    /// True if all components are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x0.is_finite() && self.y0.is_finite() && self.x1.is_finite() && self.y1.is_finite()
    }

    /// Width (0 if empty).
    #[must_use]
    pub fn width(self) -> f32 {
        (self.x1 - self.x0).max(0.0)
    }

    /// Height (0 if empty).
    #[must_use]
    pub fn height(self) -> f32 {
        (self.y1 - self.y0).max(0.0)
    }

    /// Area (0 if empty or non-finite).
    #[must_use]
    pub fn area(self) -> f32 {
        if !self.is_finite() {
            return 0.0;
        }
        self.width() * self.height()
    }

    /// True if the point is covered (min bounds inclusive, max exclusive).
    #[must_use]
    pub fn contains(self, x: f32, y: f32) -> bool {
        !self.is_empty() && x >= self.x0 && y >= self.y0 && x < self.x1 && y < self.y1
    }

    /// Bounding union — empty + r = r.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// Intersection — empty if disjoint.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let rect = Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        if rect.is_empty() { Self::EMPTY } else { rect }
    }

    /// Expands by `d` in each direction (negative = shrinks).
    #[must_use]
    pub fn expanded(self, d: f32) -> Self {
        if self.is_empty() {
            return self;
        }
        Self {
            x0: self.x0 - d,
            y0: self.y0 - d,
            x1: self.x1 + d,
            y1: self.y1 + d,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared raster vocabulary: blend modes + shareable pixel buffer.
// Owned by the foundation (photo AND video composite with these),
// re-exported by `photo-engine` for compatibility.
// ---------------------------------------------------------------------------

/// Raster blend mode.
///
/// The serialized representation is the variant name ("Normal",
/// "Multiply"…) — identical to the photo app's historical labels.
/// Numeric ids feed the backends' shaders (any
/// renumbering would break caches and projects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
    /// Display order in UIs (drop-down lists).
    pub const ALL: [BlendMode; 6] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
    ];

    /// Human-readable label.
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

    /// Numeric identifier (shaders + CPU). Do not renumber.
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

/// Shareable RGBA8 buffer with NO copy (apps derive their
/// textures from it via the `Arc`).
#[derive(Clone)]
pub struct RgbaBuf {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Shared row-major RGBA8 pixels.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_type_color_is_distinct() {
        assert_ne!(SocketType::Image.color(), SocketType::Float.color());
    }

    #[test]
    fn node_def_builder() {
        let def = NodeDefinition::new(
            "brightness_contrast",
            "Luminosité / Contraste",
            NodeCategory::Color,
        )
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .param("brightness", ParamValue::Float(0.0))
        .param("contrast", ParamValue::Float(0.0));
        assert_eq!(def.inputs.len(), 1);
        assert_eq!(def.outputs.len(), 1);
        assert_eq!(def.default_params.len(), 2);
    }

    #[test]
    fn rect_union_intersection_expansion() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let b = Rect::from_xywh(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.intersect(b), Rect::new(5.0, 5.0, 10.0, 10.0));
        assert!(
            a.intersect(Rect::from_xywh(20.0, 20.0, 5.0, 5.0))
                .is_empty()
        );
        assert!(Rect::EMPTY.is_empty());
        assert_eq!(a.expanded(2.0), Rect::new(-2.0, -2.0, 12.0, 12.0));
        assert!(a.contains(0.0, 0.0));
        assert!(!a.contains(10.0, 10.0));
        assert!((a.area() - 100.0).abs() < 0.001);
        assert!(!Rect::new(0.0, 0.0, f32::NAN, 1.0).is_finite());
    }

    #[test]
    fn blend_mode_labels_et_ids_stables() {
        assert_eq!(BlendMode::ALL.len(), 6);
        assert_eq!(BlendMode::Multiply.id(), 1);
        assert_eq!(format!("{}", BlendMode::Screen), "Screen");
        let json = serde_json::to_string(&BlendMode::Overlay).expect("serializable");
        assert_eq!(json, "\"Overlay\"");
    }

    #[test]
    fn rgba_buf_partage_sans_copie() {
        let buf = RgbaBuf::from_vec(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!((buf.width, buf.height), (2, 1));
        let clone = buf.clone();
        assert!(Arc::ptr_eq(&buf.data, &clone.data));
    }
}
