#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod playground;

#[cfg(target_arch = "wasm32")]
pub use playground::{GAME_ATLAS_SIDE, GameArt, GameFrame};

#[cfg(target_arch = "wasm32")]
mod canvas_scene;
#[cfg(target_arch = "wasm32")]
mod game_grid;
#[cfg(target_arch = "wasm32")]
mod game_renderer;
#[cfg(target_arch = "wasm32")]
mod surface_mesh;
#[cfg(target_arch = "wasm32")]
mod terrain;
#[cfg(target_arch = "wasm32")]
pub use game_renderer::{
    GameRenderer, SceneCamera, SceneResource, SceneTerrain, SceneTerrainSurface, SceneUnit,
};
#[cfg(target_arch = "wasm32")]
pub use surface_mesh::{
    ProjectedSurfaceTriangle, pick_surface_point, projected_surface_triangles,
    sample_surface_height, surface_depth_at,
};
