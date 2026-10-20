#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod playground;

#[cfg(target_arch = "wasm32")]
pub use playground::{
    AtlasAddress, GAME_ATLAS_BYTES, GAME_ATLAS_PAGE_BYTES, GAME_ATLAS_PAGES, GAME_ATLAS_SIDE,
    GameArt, GameFrame, SceneryArt, TerrainTopology, resource_frame_index,
};

#[cfg(target_arch = "wasm32")]
mod game_grid;
#[cfg(target_arch = "wasm32")]
mod game_renderer;
#[cfg(target_arch = "wasm32")]
mod surface_mesh;
#[cfg(target_arch = "wasm32")]
mod terrain;
#[cfg(target_arch = "wasm32")]
mod world_key_index;
#[cfg(target_arch = "wasm32")]
pub use game_renderer::{
    GameRenderer, SceneCamera, SceneDecoration, SceneResource, SceneTerrain,
    SceneTerrainAppearance, SceneTerrainSurface, SceneUnit, SceneUnitKind, actor_frame_index,
    resource_sprite_bounds, scene_resource_frame, scene_resource_presentation, scene_unit_animates,
    scene_unit_bounds,
};
#[cfg(target_arch = "wasm32")]
pub use surface_mesh::{
    ProjectedSurfaceTriangle, cliff_face_in_front, pick_surface_point, projected_surface_triangles,
    sample_surface_height, surface_depth_at, surface_hit_at, write_terrain_directory,
};
#[cfg(target_arch = "wasm32")]
pub use world_key_index::WorldKeyIndex;
