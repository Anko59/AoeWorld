//! Schedule world-space terrain meshes for the current and approaching view.
use super::*;
use aoe_core::TileCoord;
use aoe_rendering::{chunk_surface_halo, chunk_surface_triangles};
use std::collections::BTreeMap;

type Coordinate = (i32, i32);

// Limit preparation work, independent of the number of tiles we display.
const PREPARATION_MS: f64 = 6.0;
const MAX_PREPARATIONS: usize = 4;
const RETAINED_BYTES: usize = 20 * 1024 * 1024;

#[derive(Default)]
pub(in super::super::super) struct PatchCache {
    size: i32,
    patches: BTreeMap<Coordinate, Patch>,
}

struct Patch {
    tiles: Vec<SceneTerrain>,
    faces: Vec<ProjectedSurfaceTriangle>,
    foundations: Option<[[f64; 2]; 2]>,
    dirty: bool,
}

impl Patch {
    fn build(client: &Client, key: Coordinate, size: i32, refined: bool) -> Self {
        let origin = [key.0 * CHUNK_TILES, key.1 * CHUNK_TILES];
        let region = TileRect::from_xywh(origin[0], origin[1], CHUNK_TILES, CHUNK_TILES);
        let halo = chunk_surface_halo(size, refined);
        let support = TileRect::from_xywh(
            origin[0] - halo,
            origin[1] - halo,
            CHUNK_TILES + 2 * halo,
            CHUNK_TILES + 2 * halo,
        );
        let first = [support.min.x, support.min.y].map(|n| n.div_euclid(CHUNK_TILES));
        let last = [support.max.x - 1, support.max.y - 1].map(|n| n.div_euclid(CHUNK_TILES));
        #[cfg(feature = "frame-bench")]
        let samples = super::super::super::perf::Scope::new(30);
        let mut tiles = Vec::with_capacity((support.width() * support.height()) as usize);
        for y in first[1]..=last[1] {
            for x in first[0]..=last[0] {
                if let Some(chunk) = client.terrain_chunks.get(&(x, y)) {
                    tiles.extend(
                        chunk
                            .scene_tiles()
                            .filter(|(at, _, _)| support.contains(*at))
                            .map(|(at, tile, appearance)| {
                                terrain_scene_sample(at, tile, appearance)
                            }),
                    );
                }
            }
        }
        #[cfg(feature = "frame-bench")]
        drop(samples);
        #[cfg(feature = "frame-bench")]
        let foundation_stage = super::super::super::perf::Scope::new(31);
        let foundation_view = foundations::view_bounds(&tiles, &client.art);
        paint_foundations(client, &mut tiles, foundation_view);
        #[cfg(feature = "frame-bench")]
        drop(foundation_stage);
        #[cfg(feature = "frame-bench")]
        let mesh = super::super::super::perf::Scope::new(32);
        let faces = chunk_surface_triangles(&tiles, region, size, refined);
        #[cfg(feature = "frame-bench")]
        drop(mesh);
        tiles.retain(|tile| {
            region.contains(TileCoord::new(
                tile.position[0] as i32,
                tile.position[1] as i32,
            ))
        });
        tiles.shrink_to_fit();
        Self {
            tiles,
            faces,
            foundations: foundation_view,
            dirty: !refined,
        }
    }

    fn bytes(&self) -> usize {
        self.tiles.capacity() * size_of::<SceneTerrain>()
            + self.faces.capacity() * size_of::<ProjectedSurfaceTriangle>()
    }
}

impl PatchCache {
    pub(super) fn advance(&mut self, client: &Client, ahead: Camera) -> bool {
        let size = cell_size(client.camera);
        if self.size != size {
            self.size = size;
            self.patches.clear();
        }
        let mut jobs = Vec::new();
        for &key in client.terrain_chunks.keys() {
            let patch = self.patches.get(&key);
            if patch.is_some_and(|p| !p.dirty) {
                continue;
            }
            let visible = heights::patch_may_intersect(client, client.camera, key);
            let camera = if visible {
                client.camera
            } else if heights::patch_may_intersect(client, ahead, key) {
                ahead
            } else {
                continue;
            };
            // Finish base coverage before spending a turn on refinements.
            let priority = (patch.is_some(), !visible);
            jobs.push((
                priority,
                chunk_distance_for(key, camera, client.config),
                key,
            ));
        }
        jobs.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        let started = super::super::super::now();
        let mut visible_updated = false;
        for (index, ((refined, prefetch), _, key)) in
            jobs.into_iter().take(MAX_PREPARATIONS).enumerate()
        {
            if index > 0 && super::super::super::now() - started >= PREPARATION_MS {
                break;
            }
            self.patches
                .insert(key, Patch::build(client, key, size, refined));
            visible_updated |= !prefetch;
        }
        self.evict(client, ahead);
        visible_updated
    }

    fn evict(&mut self, client: &Client, ahead: Camera) {
        let bytes = self.resident_bytes();
        if bytes <= RETAINED_BYTES {
            return;
        }
        let mut bytes = bytes;
        let mut distant = self
            .patches
            .keys()
            .copied()
            .filter(|&key| !heights::patch_may_intersect(client, client.camera, key))
            .map(|key| (chunk_distance_for(key, ahead, client.config), key))
            .collect::<Vec<_>>();
        distant.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
        for (_, key) in distant {
            if bytes <= RETAINED_BYTES {
                break;
            }
            if let Some(patch) = self.patches.remove(&key) {
                bytes -= patch.bytes();
            }
        }
    }

    pub(in super::super) fn chunk_arrived(&mut self, key: Coordinate) {
        let halo = radius(self.size);
        for y in key.1 - halo..=key.1 + halo {
            for x in key.0 - halo..=key.0 + halo {
                if let Some(patch) = self.patches.get_mut(&(x, y)) {
                    patch.dirty = true;
                }
            }
        }
    }

    pub(in super::super) fn source_evicted(&mut self, chunks: &BTreeMap<Coordinate, CachedChunk>) {
        let retired: Vec<_> = self
            .patches
            .keys()
            .copied()
            .filter(|key| !chunks.contains_key(key))
            .collect();
        for key in retired {
            self.patches.remove(&key);
            self.chunk_arrived(key);
        }
    }

    pub(super) fn foundations_changed(&mut self, key: FoundationKey) {
        for patch in self.patches.values_mut() {
            patch.dirty |= foundations::in_view(&key, patch.foundations);
        }
    }

    pub(in super::super::super) fn resident_bytes(&self) -> usize {
        self.patches.values().map(Patch::bytes).sum()
    }

    pub(super) fn scene(&self, client: &Client) -> Option<PreparedScene> {
        let patches = client
            .terrain_chunks
            .keys()
            .filter(|&&key| heights::patch_may_intersect(client, client.camera, key))
            .map(|key| self.patches.get(key))
            .collect::<Option<Vec<_>>>()?;
        let mut terrain = Vec::new();
        let mut faces = Vec::new();
        for patch in patches {
            terrain.extend_from_slice(&patch.tiles);
            faces.extend(patch.faces.iter().filter_map(|face| {
                let mut face = *face;
                face.project_to(client.camera);
                face.intersects_viewport(client.camera.viewport)
                    .then_some(face)
            }));
        }
        Some(PreparedScene::from_triangles(client.camera, terrain, faces))
    }
}

fn radius(size: i32) -> i32 {
    if size == 0 {
        return 0;
    }
    chunk_surface_halo(size, true)
        .unsigned_abs()
        .div_ceil(CHUNK_TILES as u32) as i32
}

fn cell_size(camera: Camera) -> i32 {
    let projected_tile_area = aoe_core::ISO_TILE_WIDTH * aoe_core::ISO_TILE_HEIGHT * 0.5;
    let world_area =
        camera.viewport[0] * camera.viewport[1] / (projected_tile_area * camera.zoom * camera.zoom);
    let mut step = 1;
    // Size depends on zoom and backing only. World-aligned patch boundaries
    // and tessellation do not change under camera translation.
    while f64::from(step * step) * 4_096.0 < world_area && step < CHUNK_TILES {
        step *= 2;
    }
    step
}

#[path = "patches/tests.rs"]
#[cfg(test)]
mod tests;
