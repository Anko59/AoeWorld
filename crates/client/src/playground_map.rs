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

const MAX_REQUESTED_CHUNKS: usize = 64;
const MAX_CACHED_CHUNKS: usize = 512;
const MAX_CACHED_CHUNK_BYTES: usize = 128 * 1024 * 1024;
const MAX_VISIBLE_RESOURCE_SPRITES: usize = 1_024;
// This slice supports source heights within eight game levels of the camera
// focus. The package-backed Paris evidence stays inside this window; maps with
// a wider relief range remain explicitly incomplete until terrain metadata or
// projected surfaces can provide a larger bounded request window.
const TERRAIN_HEIGHT_MARGIN_LEVELS: f64 = 8.0;
const MAX_PICK_ITERATIONS: usize = 4;

pub(super) fn cache_status(client: &Client) -> String {
    format!(
        "terrain cache: {} / 128 MiB ({} / {MAX_CACHED_CHUNKS} chunks)",
        display_mebibytes(cached_chunk_bytes(client)),
        client.terrain_chunks.len(),
    )
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
        let requests = visible_chunks(&client)
            .into_iter()
            .filter(|coordinate| {
                !client.terrain_chunks.contains_key(coordinate)
                    && client.terrain_inflight.insert(*coordinate)
            })
            .collect::<Vec<_>>();
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
            client.terrain_inflight.remove(&(x, y));
            if client.connection_id != connection_id || client.map_content_hash != Some(map_hash) {
                return;
            }
            match result {
                Ok(chunk) => {
                    client.terrain_chunks.insert((x, y), chunk);
                    initialize_altitude_focus(&mut client);
                    evict_distant_chunks(&mut client);
                }
                Err(error) => client.status = format!("map chunk request failed: {error:?}"),
            }
        });
    }
}

pub(super) fn scene_terrain(client: &Client) -> Vec<SceneTerrain> {
    let visible = terrain_visible_tiles(client);
    let mut terrain = Vec::new();
    for chunk in client.terrain_chunks.values() {
        let (chunk_width, chunk_height) = chunk_dimensions(client, chunk.x, chunk.y);
        if chunk_width == 0 || chunk_height == 0 {
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
    let visible = terrain_visible_tiles(client);
    let mut resources = client
        .terrain_chunks
        .values()
        .flat_map(|chunk| chunk.resources.iter())
        .filter(|resource| {
            resource.tile.x >= visible.min.x
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
    let terrain = scene_terrain(client);
    let triangles = projected_surface_triangles(
        &terrain,
        aoe_rendering::SceneCamera {
            center: client.camera.center,
            zoom: client.camera.zoom,
            viewport: client.camera.viewport,
            focus_elevation_meters: client.camera.focus_elevation_meters,
        },
    );
    pick_surface_point(&triangles, screen)
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
    let focus = client.camera.focus_elevation_meters;
    let lower = client.camera.visible_tiles_at_height(
        client.config,
        8.0,
        focus - TERRAIN_HEIGHT_MARGIN_LEVELS,
    );
    let upper = client.camera.visible_tiles_at_height(
        client.config,
        8.0,
        focus + TERRAIN_HEIGHT_MARGIN_LEVELS,
    );
    TileRect::new(
        aoe_core::TileCoord::new(lower.min.x.min(upper.min.x), lower.min.y.min(upper.min.y)),
        aoe_core::TileCoord::new(upper.max.x.max(lower.max.x), upper.max.y.max(lower.max.y)),
    )
    .clamp(client.config.width_tiles, client.config.height_tiles)
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
    client.camera.focus_elevation_meters = f64::from(tile.game_height_level);
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

fn visible_chunks(client: &Client) -> Vec<(i32, i32)> {
    let visible = terrain_visible_tiles(client);
    let min_x = visible.min.x.div_euclid(CHUNK_TILES);
    let min_y = visible.min.y.div_euclid(CHUNK_TILES);
    let max_x = visible.max.x.saturating_sub(1).div_euclid(CHUNK_TILES);
    let max_y = visible.max.y.saturating_sub(1).div_euclid(CHUNK_TILES);
    let mut chunks = (min_y..=max_y)
        .flat_map(|y| (min_x..=max_x).map(move |x| (x, y)))
        .collect::<Vec<_>>();
    chunks.sort_by(|left, right| {
        chunk_distance(*left, client)
            .total_cmp(&chunk_distance(*right, client))
            .then(left.cmp(right))
    });
    chunks.truncate(MAX_REQUESTED_CHUNKS);
    chunks
}

fn chunk_distance((x, y): (i32, i32), client: &Client) -> f64 {
    let center_x = f64::from(x * CHUNK_TILES + CHUNK_TILES / 2) - client.camera.center[0];
    let center_y = f64::from(y * CHUNK_TILES + CHUNK_TILES / 2) - client.camera.center[1];
    center_x.mul_add(center_x, center_y * center_y)
}

fn evict_distant_chunks(client: &mut Client) {
    let mut cached_bytes = cached_chunk_bytes(client);
    if client.terrain_chunks.len() <= MAX_CACHED_CHUNKS && cached_bytes <= MAX_CACHED_CHUNK_BYTES {
        return;
    }
    let mut coordinates = client.terrain_chunks.keys().copied().collect::<Vec<_>>();
    coordinates.sort_by(|left, right| {
        chunk_distance(*right, client)
            .total_cmp(&chunk_distance(*left, client))
            .then(right.cmp(left))
    });
    for coordinate in coordinates {
        if client.terrain_chunks.len() <= MAX_CACHED_CHUNKS
            && cached_bytes <= MAX_CACHED_CHUNK_BYTES
        {
            break;
        }
        if let Some(chunk) = client.terrain_chunks.remove(&coordinate) {
            cached_bytes = cached_bytes.saturating_sub(chunk_resident_bytes(&chunk));
        }
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
