//! Document LayerTree — facade re-exporting submodules.
pub mod compositing;
pub mod dirty;
pub mod drag_proxy;
pub mod model;
pub mod preview_surface;
pub mod scaled;
pub mod scheduler;
#[cfg(test)]
mod tests;
pub mod tile_cache;
pub mod tree;
pub mod worker;

pub(crate) use compositing::{preview_buf, thumb_buf};
pub use dirty::DirtyRegion;
pub use drag_proxy::ProxyPart;
pub use model::{
    AdjustmentLayer, Appearance, BlendMode, FilterLayer, FilterNode, GroupLayer, LayerMask,
    LayerNode, PixelLayer, RgbaBuf, Transform2D, VectorLayer, next_appearance_version,
};
pub use preview_surface::{PreviewSurface, PreviewSurfaceStats};
pub use scaled::ScaledAppearances;
pub use scheduler::{
    Camera, RenderPriorityContext, RenderRequest, TilePriority, TileRenderRequest,
    schedule_viewport,
};
pub use tile_cache::{DocTileKey, TILE_CACHE_FLAGS_NONE, TileCache, TileCacheStats, get_or_render};
pub use tree::Document;
pub use worker::{AssembledView, RenderWorker, RenderWorkerStats, ScopeGeom};
