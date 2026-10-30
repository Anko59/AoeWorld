#[cfg(any(not(target_arch = "wasm32"), test, feature = "shader-source"))]
pub mod shader_data;

mod player;
pub use player::{PLAYER_COLOURS, SceneUnitVisual, player_colour};

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod playground;

#[cfg(target_arch = "wasm32")]
pub use playground::{
    AtlasAddress, BuildingArt, BuildingDamage, BuildingPart, GAME_ATLAS_BYTES,
    GAME_ATLAS_PAGE_BYTES, GAME_ATLAS_PAGES, GAME_ATLAS_SIDE, GameArt, GameFrame,
    MAX_GAME_ATLAS_PAGES, SceneryArt, TerrainTopology, atlas_mask_page, atlas_pages,
    resource_frame_index,
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
    AtlasSourceCopy, AtlasUpload, GameRenderer, SceneCamera, SceneDecoration, SceneResource,
    SceneTerrain, SceneTerrainAppearance, SceneTerrainSurface, SceneUnit, SceneUnitKind,
    actor_frame_index, apply_building_foundations, resource_sprite_bounds, scene_building_hit,
    scene_building_visible, scene_native_frames, scene_native_prefetch,
    scene_resource_animation_frame, scene_resource_frame, scene_resource_native_frames,
    scene_resource_presentation, scene_unit_animates, scene_unit_bounds, scene_unit_depth,
    scene_unit_render_key,
};
#[cfg(target_arch = "wasm32")]
pub use surface_mesh::{
    ProjectedSurfaceTriangle, cliff_face_in_front, pick_surface_point, projected_surface_triangles,
    sample_surface_height, scene_cliff_native_frames, scene_terrain_frames, surface_depth_at,
    surface_hit_at, terrain_directory_rows, write_terrain_directory, write_terrain_directory_page,
};
#[cfg(target_arch = "wasm32")]
pub use world_key_index::WorldKeyIndex;
