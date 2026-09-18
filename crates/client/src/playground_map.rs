use super::Client;
use aoe_map::{CHUNK_TILES, Chunk, CompactChunk, GroundMaterial};
use aoe_rendering::SceneTerrain;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::Response;

const MAX_REQUESTED_CHUNKS: usize = 64;
const MAX_CACHED_CHUNKS: usize = 96;

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
    if client.terrain_chunks.len() <= MAX_CACHED_CHUNKS {
        return;
    }
    let mut coordinates = client.terrain_chunks.keys().copied().collect::<Vec<_>>();
    coordinates.sort_by(|left, right| {
        chunk_distance(*right, client)
            .total_cmp(&chunk_distance(*left, client))
            .then(right.cmp(left))
    });
    for coordinate in coordinates
        .into_iter()
        .take(client.terrain_chunks.len() - MAX_CACHED_CHUNKS)
    {
        client.terrain_chunks.remove(&coordinate);
    }
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
