use super::Client;
use aoe_map::{CHUNK_TILES, Chunk, CompactChunk, GroundMaterial, ResourceNode, Tile};
use aoe_rendering::{SceneResource, SceneTerrain};
use std::{cell::RefCell, mem::size_of, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::Response;

const MAX_REQUESTED_CHUNKS: usize = 64;
const MAX_CACHED_CHUNKS: usize = 512;
const MAX_CACHED_CHUNK_BYTES: usize = 128 * 1024 * 1024;
const MAX_VISIBLE_RESOURCE_SPRITES: usize = 1_024;

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
    let world = client.camera.screen_to_world(pointer);
    let x = world[0].floor() as i32;
    let y = world[1].floor() as i32;
    let Some(chunk) = client
        .terrain_chunks
        .get(&(x.div_euclid(CHUNK_TILES), y.div_euclid(CHUNK_TILES)))
    else {
        return format!("tile {x}, {y}: terrain loading");
    };
    let local_x = x.rem_euclid(CHUNK_TILES) as usize;
    let local_y = y.rem_euclid(CHUNK_TILES) as usize;
    let Some(tile) = chunk.tiles.get(local_y * CHUNK_TILES as usize + local_x) else {
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
                    evict_distant_chunks(&mut client);
                }
                Err(error) => client.status = format!("map chunk request failed: {error:?}"),
            }
        });
    }
}

pub(super) fn scene_terrain(client: &Client) -> Vec<SceneTerrain> {
    let visible = client
        .camera
        .visible_tiles(client.config, 8.0)
        .clamp(client.config.width_tiles, client.config.height_tiles);
    let loaded_tiles = client
        .terrain_chunks
        .values()
        .map(|chunk| chunk.tiles.len())
        .sum::<usize>();
    let stride = (loaded_tiles.div_ceil(4_096) as f64).sqrt().ceil().max(1.0) as usize;
    let mut terrain = Vec::with_capacity(loaded_tiles.div_ceil(stride * stride));
    for chunk in client.terrain_chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let local_x = index as i32 % CHUNK_TILES;
            let local_y = index as i32 / CHUNK_TILES;
            if (local_x as usize % stride) != 0 || (local_y as usize % stride) != 0 {
                continue;
            }
            let x = chunk.x * CHUNK_TILES + local_x;
            let y = chunk.y * CHUNK_TILES + local_y;
            if x < visible.min.x || x >= visible.max.x || y < visible.min.y || y >= visible.max.y {
                continue;
            }
            terrain.push(SceneTerrain {
                position: [f64::from(x), f64::from(y)],
                material: terrain_material(tile.material),
            });
        }
    }
    terrain
}

pub(super) fn scene_resources(client: &Client) -> Vec<SceneResource> {
    let visible = client
        .camera
        .visible_tiles(client.config, 8.0)
        .clamp(client.config.width_tiles, client.config.height_tiles);
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
            position: [f64::from(resource.tile.x), f64::from(resource.tile.y)],
            kind: resource.kind as u8,
            visual_variant: resource.visual_variant,
        })
        .collect::<Vec<_>>();
    resources.sort_by_key(|resource| resource.id);
    resources.truncate(MAX_VISIBLE_RESOURCE_SPRITES);
    resources
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
    let visible = client
        .camera
        .visible_tiles(client.config, 8.0)
        .clamp(client.config.width_tiles, client.config.height_tiles);
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
mod tests {
    use super::*;
    use aoe_map::MapChunkGenerator;

    #[test]
    fn resident_chunk_measurement_counts_the_struct_and_owned_buffers() {
        let chunk = MapChunkGenerator::new([0; 32], 1, 32).chunk(0, 0);
        assert_eq!(
            chunk_resident_bytes(&chunk),
            size_of::<Chunk>()
                + chunk.tiles.capacity() * size_of::<Tile>()
                + chunk.resources.capacity() * size_of::<ResourceNode>()
        );
    }

    #[test]
    fn decoded_cache_limit_matches_the_product_budget() {
        assert_eq!(MAX_CACHED_CHUNKS, 512);
        assert_eq!(MAX_CACHED_CHUNK_BYTES, 128 * 1024 * 1024);
    }
}
