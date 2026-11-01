//! Apply ready visible cells, then return bounded requests for the next turn.
use super::*;
use aoe_assets::catalog::packing::residency::{GUTTER, TILE_SIDE};

pub(crate) struct Request {
    index: u16,
    chunk: FrameChunk,
    completed: Rc<RefCell<Vec<(u16, Result<Vec<u8>, JsValue>)>>>,
}
impl Request {
    pub(crate) fn spawn(self) {
        wasm_bindgen_futures::spawn_local(async move {
            let result = fetch_chunk(&self.chunk.file, self.chunk.bytes)
                .await
                .map(|value| Uint8Array::new(&value).to_vec());
            self.completed.borrow_mut().push((self.index, result));
        });
    }
}
fn address(p: Placement) -> AtlasAddress {
    let side = GAME_ATLAS_SIDE as f32;
    AtlasAddress {
        page: u32::from(p.page),
        uv: [
            f32::from(p.x) / side,
            f32::from(p.y) / side,
            f32::from(p.width) / side,
            f32::from(p.height) / side,
        ],
    }
}
impl NativeFrames {
    pub(crate) fn update(
        &mut self,
        required: &[u32],
        prefetch: &[u32],
        keep: &[u32],
        art: &mut GameArt,
        renderer: &mut aoe_rendering::GameRenderer,
    ) -> Result<(bool, Vec<Request>), JsValue> {
        if self.failed {
            return Ok((false, Vec::new()));
        }
        let result = self.update_ready(required, prefetch, keep, art, renderer);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    #[inline(never)]
    fn update_ready(
        &mut self,
        required: &[u32],
        prefetch: &[u32],
        keep: &[u32],
        art: &mut GameArt,
        renderer: &mut aoe_rendering::GameRenderer,
    ) -> Result<(bool, Vec<Request>), JsValue> {
        let completed = std::mem::take(&mut *self.completed.borrow_mut());
        for (index, bytes) in completed {
            match bytes {
                Ok(bytes) => self.accept(index, bytes)?,
                Err(_) => {
                    if !self.inflight.remove(&index) {
                        return Err(error("Unexpected native chunk response"));
                    }
                    self.retries[index as usize].fail(retry::now());
                }
            }
        }
        if art.resident.len() != self.cells.len() {
            return Err(error("Native residency inventory changed"));
        }
        for &id in required.iter().chain(prefetch).chain(keep) {
            self.cell(id)?;
        }
        let keys: Vec<_> = required
            .iter()
            .chain(keep)
            .map(|&id| u64::from(id))
            .collect();
        self.cache.begin_frame(&keys);
        let mut changed = false;
        let mut uploads = 0;
        let mut uploaded_bytes = 0;
        if renderer.atlas_updates_ready().map_err(error)? {
            for (id, current) in required
                .iter()
                .map(|&id| (id, true))
                .chain(prefetch.iter().map(|&id| (id, false)))
            {
                if !renderer.atlas_updates_ready().map_err(error)? {
                    break;
                }
                if let Some(p) = self.cache.lookup(u64::from(id)) {
                    art.resident[id as usize - 1] = Some(address(p));
                    continue;
                }
                let cell = *self.cell(id)?;
                let width = (cell.size[0] + 2 * GUTTER).div_ceil(TILE_SIDE) * TILE_SIDE;
                let height = (cell.size[1] + 2 * GUTTER).div_ceil(TILE_SIDE) * TILE_SIDE;
                let bytes = usize::from(width) * usize::from(height) * 8;
                if uploads >= 16 || (uploads > 0 && uploaded_bytes + bytes > 8 * 1024 * 1024) {
                    break;
                }
                if self.encoded[cell.location.chunk as usize].is_none() {
                    continue;
                }
                let previous_layers = self.cache.layer_count();
                let reserved = self
                    .cache
                    .reserve(
                        u64::from(id),
                        cell.size,
                        current,
                        renderer.atlas_growth_ready().map_err(error)?,
                    )
                    .map_err(error)?;
                for evicted in reserved.evicted {
                    art.resident[evicted as usize - 1] = None;
                    changed = true;
                }
                let Some(p) = reserved.placement else {
                    continue;
                };
                if self.cache.layer_count() != previous_layers {
                    renderer
                        .grow_game_atlas(u32::from(self.cache.layer_count()))
                        .map_err(error)?;
                }
                // Admission precedes allocation/decompression, so pressure
                // cannot repeatedly decode cells that cannot be uploaded.
                let planes = self
                    .decode(id)?
                    .ok_or_else(|| error("Admitted native chunk disappeared"))?;
                let pixels = pixels::compose(&planes, p, cell.owned, cell.team)?;
                for (layer, pixels) in [p.page, p.page + 1].into_iter().zip(&pixels) {
                    renderer
                        .update_game_atlas(
                            u32::from(layer),
                            [u32::from(p.x - GUTTER), u32::from(p.y - GUTTER)],
                            [u32::from(width), u32::from(height)],
                            pixels,
                        )
                        .map_err(error)?;
                }
                art.resident[id as usize - 1] = Some(address(p));
                changed = true;
                uploads += 1;
                uploaded_bytes += bytes;
            }
        }
        self.directory_pending = false;
        if art
            .terrain_topology
            .iter()
            .any(|t| matches!(t, Some(aoe_rendering::TerrainTopology::Dds { .. })))
        {
            let rows = aoe_rendering::terrain_directory_rows(art).map_err(error)?;
            if rows != self.directory {
                self.directory_pending = true;
                if renderer.atlas_updates_ready().map_err(error)? {
                    renderer
                        .update_game_atlas(0, [0, 0], [2048, 2], &rows)
                        .map_err(error)?;
                    self.directory = rows;
                    self.directory_pending = false;
                    changed = true;
                }
            }
        }
        let missing: Vec<_> = required
            .iter()
            .chain(prefetch)
            .copied()
            .filter(|&id| self.cache.lookup(u64::from(id)).is_none())
            .collect();
        let requests = self
            .requests(&missing)?
            .into_iter()
            .map(|index| Request {
                index,
                chunk: self.chunks[index as usize].clone(),
                completed: self.completed.clone(),
            })
            .collect();
        Ok((changed, requests))
    }

    pub(crate) fn pin_presented(&mut self, frames: &[u32]) {
        self.cache
            .protect(&frames.iter().map(|&id| u64::from(id)).collect::<Vec<_>>());
    }
}
