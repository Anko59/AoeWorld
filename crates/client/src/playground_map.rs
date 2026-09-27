use super::Client;
use aoe_core::{Camera, ScreenPoint, TileRect};
use aoe_map::{CHUNK_TILES, Chunk, CompactChunk, GroundMaterial, ResourceNode, Tile};
use aoe_rendering::{
    SceneResource, SceneTerrain, SceneTerrainSurface, pick_surface_point,
    projected_surface_triangles, sample_surface_height,
};
use std::{cell::RefCell, mem::size_of, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::Response;

#[path = "playground_map/eviction.rs"]
mod eviction;
#[path = "playground_map/heights.rs"]
pub(crate) mod heights;
#[path = "playground_map/state.rs"]
pub(crate) mod state;
#[path = "playground_map/ui.rs"]
pub(crate) mod ui;
pub(super) use eviction::evict_distant_chunks_with_limits;
use heights::{
    include_chunk_height_bounds, refresh_chunk_height_bounds, visible_tiles_for_height_bounds,
};

pub(super) fn install_fixture_chunk(client: &mut Client, chunk: &Chunk) {
    include_chunk_height_bounds(client, chunk);
}

const MAX_REQUESTED_CHUNKS: usize = 64;
const MAX_CACHED_CHUNKS: usize = 512;
const MAX_CACHED_CHUNK_BYTES: usize = 128 * 1024 * 1024;
const MAX_VISIBLE_RESOURCE_SPRITES: usize = 1_024;
const MAX_PICK_ITERATIONS: usize = 4;

pub(super) fn cache_status(client: &Client) -> String {
    format!(
        "terrain cache: {} / 128 MiB ({} / {MAX_CACHED_CHUNKS} chunks)",
        display_mebibytes(cached_chunk_bytes(client)),
        client.terrain_chunks.len(),
    )
}

pub(super) fn clear_terrain_cache(client: &mut Client) {
    client.terrain_chunks.clear();
    client.terrain_discovered.clear();
    client.terrain_bounds = Default::default();
    client.terrain_height_bounds = None;
    client.terrain_resident_height_bounds = None;
    client.terrain_inflight.clear();
}

pub(super) fn inspection_label(client: &Client) -> String {
    let Some(pointer) = client.pointer else {
        return "hover a loaded tile to inspect terrain".to_owned();
    };
    let Some(world) = world_at_screen(client, pointer) else {
        return "terrain unavailable at pointer".to_owned();
    };
    let x = world[0].floor() as i32;
    let y = world[1].floor() as i32;
    if client.map_content_hash.is_none() {
        return format!("tile {x}, {y}: diagnostic ground");
    }
    if !client
        .terrain_chunks
        .contains_key(&(x.div_euclid(CHUNK_TILES), y.div_euclid(CHUNK_TILES)))
    {
        return format!("tile {x}, {y}: terrain loading");
    }
    let Some(tile) = terrain_tile(client, x, y) else {
        return format!("tile {x}, {y}: terrain unavailable");
    };
    format!(
        "tile {x}, {y}: movement level {} m · geographic height {:.2} m · {:?} · {:?} water · {:?} elevation · {}",
        tile.game_height_level,
        f64::from(tile.geographic_height_centimeters) / 100.0,
        tile.material,
        tile.water,
        tile.elevation_provenance,
        if tile.passable { "passable" } else { "blocked" },
    )
}

pub(super) fn request_visible(shared: Rc<RefCell<Client>>) {
    let (connection_id, map_hash, requests) = {
        let mut client = shared.borrow_mut();
        let Some(content_hash) = client.map_content_hash else {
            return;
        };
        if !heights::ensure_bounds(shared.clone(), &mut client, content_hash) {
            return;
        }
        let requests = visible_chunks(&mut client);
        (client.connection_id, content_hash, requests)
    };
    let content_hash = map_hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    for (x, y) in requests {
        let shared = shared.clone();
        let content_hash = content_hash.clone();
        spawn_local(async move {
            let result = fetch_chunk(&content_hash, x, y).await;
            let mut client = shared.borrow_mut();
            if client.connection_id != connection_id || client.map_content_hash != Some(map_hash) {
                return;
            }
            client.terrain_inflight.remove(&(x, y));
            match result {
                Ok(chunk) => {
                    include_chunk_height_bounds(&mut client, &chunk);
                    client.terrain_discovered.insert((x, y));
                    client.terrain_chunks.insert((x, y), chunk);
                    initialize_altitude_focus(&mut client);
                    evict_distant_chunks(&mut client);
                }
                Err(error) => {
                    client.status = format!("map chunk request failed: {error:?}");
                    heights::defer_failed_chunks(&mut client);
                }
            }
        });
    }
}

pub(super) fn scene_terrain(client: &Client) -> Vec<SceneTerrain> {
    let visible = resident_visible_tiles(client);
    let mut terrain = Vec::new();
    for chunk in client.terrain_chunks.values() {
        let (chunk_width, chunk_height) = chunk_dimensions(client, chunk.x, chunk.y);
        if chunk_width == 0
            || chunk_height == 0
            || !heights::resident_may_be_visible(client, (chunk.x, chunk.y))
        {
            continue;
        }
        let chunk_min_x = chunk.x * CHUNK_TILES;
        let chunk_min_y = chunk.y * CHUNK_TILES;
        let chunk_max_x = chunk_min_x + chunk_width as i32;
        let chunk_max_y = chunk_min_y + chunk_height as i32;
        if chunk_max_x <= visible.min.x
            || chunk_min_x >= visible.max.x
            || chunk_max_y <= visible.min.y
            || chunk_min_y >= visible.max.y
        {
            continue;
        }
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let local_x = index % chunk_width;
            let local_y = index / chunk_width;
            if local_y >= chunk_height {
                continue;
            }
            let x = chunk.x * CHUNK_TILES + local_x as i32;
            let y = chunk.y * CHUNK_TILES + local_y as i32;
            if x < visible.min.x || x >= visible.max.x || y < visible.min.y || y >= visible.max.y {
                continue;
            }
            terrain.push(SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: terrain_material(tile.material),
                elevation_meters: tile_center_elevation(tile),
                surface: SceneTerrainSurface {
                    corner_game_height_levels: tile.surface.corner_game_height_levels,
                    kind: tile.surface.kind as u8,
                    triangulation: tile.surface.triangulation as u8,
                    water: tile.water as u8,
                },
            });
        }
    }
    terrain
}

pub(super) fn scene_resources(client: &Client) -> Vec<SceneResource> {
    let visible = resident_visible_tiles(client);
    let mut resources = client
        .terrain_chunks
        .values()
        .filter(|chunk| heights::resident_may_be_visible(client, (chunk.x, chunk.y)))
        .flat_map(|chunk| chunk.resources.iter())
        .filter(|resource| {
            client.resources.visible(resource.id)
                && resource.tile.x >= visible.min.x
                && resource.tile.x < visible.max.x
                && resource.tile.y >= visible.min.y
                && resource.tile.y < visible.max.y
        })
        .map(|resource| SceneResource {
            id: resource.id,
            position: [
                f64::from(resource.tile.x) + 0.5,
                f64::from(resource.tile.y) + 0.5,
            ],
            kind: resource.kind as u8,
            visual_variant: resource.visual_variant,
            elevation_meters: elevation_at_tile(client, resource.tile.x, resource.tile.y),
        })
        .collect::<Vec<_>>();
    resources.sort_by_key(|resource| resource.id);
    resources.truncate(MAX_VISIBLE_RESOURCE_SPRITES);
    resources
}

pub(super) fn elevation_at_world(client: &Client, world: [f64; 2]) -> f64 {
    let x = world[0].floor() as i32;
    let y = world[1].floor() as i32;
    terrain_tile(client, x, y).map_or(0.0, |tile| {
        surface_elevation(tile, world[0] - f64::from(x), world[1] - f64::from(y))
    })
}

pub(super) fn screen_position(client: &Client, world: [f64; 2]) -> ScreenPoint {
    client
        .camera
        .world_to_screen_at_height(world, elevation_at_world(client, world))
}

pub(super) fn world_at_screen(client: &Client, screen: ScreenPoint) -> Option<[f64; 2]> {
    if client.map_content_hash.is_none() {
        return world_at_screen_with_height(client.camera, screen, |_, _| Some(0.0));
    }
    let triangles = projected_terrain_mesh(client);
    pick_surface_point(&triangles, screen)
}

pub(super) fn surface_depth_at_screen(client: &Client, screen: ScreenPoint) -> Option<f64> {
    if client.map_content_hash.is_none() {
        return None;
    }
    let triangles = projected_terrain_mesh(client);
    aoe_rendering::surface_depth_at(&triangles, screen)
}

fn projected_terrain_mesh(client: &Client) -> Vec<aoe_rendering::ProjectedSurfaceTriangle> {
    projected_surface_triangles(
        &scene_terrain(client),
        aoe_rendering::SceneCamera {
            center: client.camera.center,
            zoom: client.camera.zoom,
            viewport: client.camera.viewport,
            focus_elevation_meters: client.camera.focus_elevation_meters,
        },
    )
}

fn world_at_screen_with_height(
    camera: Camera,
    screen: ScreenPoint,
    mut height_at: impl FnMut(i32, i32) -> Option<f64>,
) -> Option<[f64; 2]> {
    let mut world = camera.screen_to_world_at_height(screen, camera.focus_elevation_meters);
    for _ in 0..MAX_PICK_ITERATIONS {
        let x = world[0].floor() as i32;
        let y = world[1].floor() as i32;
        let elevation = height_at(x, y)?;
        let next = camera.screen_to_world_at_height(screen, elevation);
        if (next[0] - world[0]).abs() < 0.001 && (next[1] - world[1]).abs() < 0.001 {
            return Some(next);
        }
        world = next;
    }
    None
}

fn elevation_at_tile(client: &Client, x: i32, y: i32) -> f64 {
    terrain_tile(client, x, y).map_or(0.0, tile_center_elevation)
}

fn tile_center_elevation(tile: &Tile) -> f64 {
    sample_surface_height(
        tile.surface.corner_game_height_levels,
        tile.surface.triangulation as u8,
        0.5,
        0.5,
    )
}

fn surface_elevation(tile: &Tile, x: f64, y: f64) -> f64 {
    sample_surface_height(
        tile.surface.corner_game_height_levels,
        tile.surface.triangulation as u8,
        x,
        y,
    )
}

fn terrain_tile(client: &Client, x: i32, y: i32) -> Option<&Tile> {
    if x < 0 || y < 0 || x >= client.config.width_tiles || y >= client.config.height_tiles {
        return None;
    }
    let chunk_x = x.div_euclid(CHUNK_TILES);
    let chunk_y = y.div_euclid(CHUNK_TILES);
    let chunk = client.terrain_chunks.get(&(chunk_x, chunk_y))?;
    chunk_tile_index(
        client.config.width_tiles,
        client.config.height_tiles,
        chunk,
        x,
        y,
    )
    .and_then(|index| chunk.tiles.get(index))
}

pub(super) fn terrain_visible_tiles(client: &Client) -> TileRect {
    visible_tiles_for_height_bounds(
        client.camera,
        client.config,
        client.camera.focus_elevation_meters,
        client.terrain_height_bounds,
    )
}

pub(super) fn resident_visible_tiles(client: &Client) -> TileRect {
    visible_tiles_for_height_bounds(
        client.camera,
        client.config,
        client.camera.focus_elevation_meters,
        client.terrain_resident_height_bounds,
    )
}

pub(super) fn center_on_world(client: &mut Client, position: [f64; 2]) {
    client.camera.center = position;
    // If the destination has not loaded, its first chunk finishes centering.
    client.focus_map_hash = None;
    initialize_altitude_focus(client);
}

fn initialize_altitude_focus(client: &mut Client) {
    let Some(map_hash) = client.map_content_hash else {
        return;
    };
    if client.focus_map_hash == Some(map_hash) {
        return;
    }
    let x = client.camera.center[0].floor() as i32;
    let y = client.camera.center[1].floor() as i32;
    let Some(tile) = terrain_tile(client, x, y) else {
        return;
    };
    client.camera.focus_elevation_meters = surface_elevation(
        tile,
        client.camera.center[0] - f64::from(x),
        client.camera.center[1] - f64::from(y),
    );
    client.focus_map_hash = Some(map_hash);
}

fn chunk_dimensions(client: &Client, chunk_x: i32, chunk_y: i32) -> (usize, usize) {
    (
        chunk_axis_len(client.config.width_tiles, chunk_x),
        chunk_axis_len(client.config.height_tiles, chunk_y),
    )
}

fn chunk_axis_len(total_tiles: i32, chunk: i32) -> usize {
    let start = i64::from(chunk) * i64::from(CHUNK_TILES);
    let remaining = i64::from(total_tiles).saturating_sub(start);
    usize::try_from(remaining.clamp(0, i64::from(CHUNK_TILES))).unwrap_or(0)
}

fn chunk_tile_index(
    width_tiles: i32,
    height_tiles: i32,
    chunk: &Chunk,
    x: i32,
    y: i32,
) -> Option<usize> {
    if x < 0 || y < 0 || x >= width_tiles || y >= height_tiles {
        return None;
    }
    let (chunk_width, chunk_height) = (
        chunk_axis_len(width_tiles, chunk.x),
        chunk_axis_len(height_tiles, chunk.y),
    );
    let local_x = x.rem_euclid(CHUNK_TILES) as usize;
    let local_y = y.rem_euclid(CHUNK_TILES) as usize;
    (local_x < chunk_width && local_y < chunk_height).then_some(local_y * chunk_width + local_x)
}

fn terrain_material(material: GroundMaterial) -> u8 {
    match material {
        GroundMaterial::TemperateGrass
        | GroundMaterial::LushGrass
        | GroundMaterial::ForestFloor => 0,
        GroundMaterial::DryGrass
        | GroundMaterial::Mud
        | GroundMaterial::Snow
        | GroundMaterial::Ice => 1,
        GroundMaterial::Dirt => 2,
        GroundMaterial::Sand | GroundMaterial::Shore => 3,
        GroundMaterial::Rock => 4,
        GroundMaterial::Water => 5,
    }
}

fn visible_chunks(client: &mut Client) -> Vec<(i32, i32)> {
    let available = MAX_REQUESTED_CHUNKS.saturating_sub(client.terrain_inflight.len());
    let requests = heights::request_candidates(client, available);
    client.terrain_inflight.extend(requests.iter().copied());
    requests
}

fn chunk_distance_for((x, y): (i32, i32), camera: Camera, _config: aoe_core::WorldConfig) -> f64 {
    let center_x = f64::from(x * CHUNK_TILES + CHUNK_TILES / 2) - camera.center[0];
    let center_y = f64::from(y * CHUNK_TILES + CHUNK_TILES / 2) - camera.center[1];
    center_x.mul_add(center_x, center_y * center_y)
}

fn evict_distant_chunks(client: &mut Client) {
    let preferred = heights::visible_residents(client);
    let (removed, _) = evict_distant_chunks_with_limits(
        &mut client.terrain_chunks,
        &mut client.terrain_discovered,
        client.camera,
        client.config,
        MAX_CACHED_CHUNKS,
        MAX_CACHED_CHUNK_BYTES,
        &preferred,
    );
    if removed {
        refresh_chunk_height_bounds(client);
    }
}

fn cached_chunk_bytes(client: &Client) -> usize {
    client
        .terrain_chunks
        .values()
        .map(chunk_resident_bytes)
        .sum()
}

fn chunk_resident_bytes(chunk: &Chunk) -> usize {
    size_of::<Chunk>()
        .saturating_add(chunk.tiles.capacity().saturating_mul(size_of::<Tile>()))
        .saturating_add(
            chunk
                .resources
                .capacity()
                .saturating_mul(size_of::<ResourceNode>()),
        )
}

fn display_mebibytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

async fn fetch_chunk(content_hash: &str, x: i32, y: i32) -> Result<Chunk, JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let response: Response =
        JsFuture::from(window.fetch_with_str(&format!("/maps/{content_hash}/chunks/{x}/{y}")))
            .await?
            .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    let body = JsFuture::from(response.text()?).await?;
    let compact: CompactChunk =
        serde_json::from_str(&body.as_string().ok_or("map chunk response was not text")?)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
    compact
        .decode()
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[cfg(test)]
#[path = "playground_map_tests.rs"]
mod tests;
