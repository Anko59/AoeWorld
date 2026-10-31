//! Bounded native animation chunks and their visible GPU working set.
use super::*;
use aoe_assets::{
    catalog::packing::residency::FrameCache,
    pack::{
        compressed,
        frame_chunks::{FrameChunk, FrameLocation},
    },
};
use std::collections::{BTreeMap, BTreeSet};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::wasm_bindgen;

mod pixels;
mod prepare;
mod runtime;
pub(super) use prepare::prepare;
const ENCODED_BYTES: usize = 96 * 1024 * 1024;
const INFLIGHT_BYTES: usize = 64 * 1024 * 1024;
const MAX_REQUESTS: usize = 4;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Cell {
    pub location: FrameLocation,
    pub size: [u16; 2],
    pub owned: bool,
    pub team: [u8; 3],
}
struct Encoded {
    bytes: Vec<u8>,
    touched: u64,
}
pub(crate) struct NativeFrames {
    chunks: Vec<FrameChunk>,
    cells: Vec<Cell>,
    cache: FrameCache,
    encoded: Vec<Option<Encoded>>,
    inflight: BTreeSet<u16>,
    clock: u64,
    completed: Rc<RefCell<Vec<(u16, Result<Vec<u8>, JsValue>)>>>,
    failed: bool,
    directory: Vec<u8>,
    directory_pending: bool,
}

fn valid_name(name: &str) -> bool {
    let file = name.strip_prefix("buildings/").unwrap_or(name);
    !file.is_empty()
        && !file.contains('/')
        && !file.contains('\\')
        && file.ends_with(".bin")
        && file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl NativeFrames {
    pub(super) fn new(
        chunks: Vec<FrameChunk>,
        cells: Vec<Cell>,
        base: u16,
    ) -> Result<Self, JsValue> {
        use aoe_assets::pack::frame_chunks::{MAX_CHUNKS, MAX_TOTAL_BYTES};
        if chunks.len() > MAX_CHUNKS || cells.len() > MAX_SELECTED_FRAMES {
            return Err(error("Native metadata exceeds bounds"));
        }
        let mut names = BTreeSet::new();
        let mut total = 0_u64;
        for chunk in &chunks {
            if !valid_name(&chunk.file)
                || !names.insert(&chunk.file)
                || chunk.bytes == 0
                || chunk.bytes as usize > compressed::MAX_ENCODED_FRAME_BYTES
                || chunk.hash.len() != 64
                || !chunk.hash.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(error("Invalid native chunk metadata"));
            }
            total += u64::from(chunk.bytes);
        }
        if total > MAX_TOTAL_BYTES as u64 {
            return Err(error("Native chunks exceed byte budget"));
        }
        for cell in &cells {
            let chunk = chunks
                .get(usize::from(cell.location.chunk))
                .ok_or("Invalid native chunk reference")?;
            if cell
                .size
                .into_iter()
                .any(|n| n == 0 || n > aoe_assets::catalog::packing::MAX_FRAME_EXTENT)
                || cell.location.length == 0
                || u64::from(cell.location.offset) + u64::from(cell.location.length)
                    > u64::from(chunk.bytes)
            {
                return Err(error("Invalid native cell metadata"));
            }
        }
        // Total colour/mask storage, including startup layers, stays within
        // 512 MiB; growth temporarily retains at most one previous texture.
        let limit = (512 * 1024 * 1024 / GAME_ATLAS_PAGE_BYTES)
            .min(aoe_rendering::MAX_GAME_ATLAS_PAGES as usize) as u16;
        let pairs = limit.saturating_sub(base) / 2;
        let cache = FrameCache::new(base, pairs.min(4), limit).map_err(error)?;
        let mut encoded = Vec::new();
        encoded.resize_with(chunks.len(), || None);
        Ok(Self {
            chunks,
            cells,
            cache,
            encoded,
            inflight: BTreeSet::new(),
            clock: 0,
            completed: Rc::default(),
            failed: false,
            directory: Vec::new(),
            directory_pending: false,
        })
    }
    pub(crate) fn layer_count(&self) -> u16 {
        self.cache.layer_count()
    }
    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }
    pub(crate) fn failed(&self) -> bool {
        self.failed
    }
    pub(crate) fn ready(&self) -> bool {
        !self.failed && !self.directory_pending
    }
    /// Encoded payload, reserved network bytes, metadata payload and resident
    /// frame count. Reservations are a budget, not additional owned memory.
    pub(crate) fn memory_usage(&self, art: &GameArt) -> [usize; 4] {
        use std::mem::size_of;
        let completed = self.completed.borrow();
        let bytes = self
            .encoded
            .iter()
            .flatten()
            .map(|e| e.bytes.capacity())
            .sum::<usize>()
            + completed
                .iter()
                .filter_map(|(_, r)| r.as_ref().ok())
                .map(Vec::capacity)
                .sum::<usize>();
        let metadata = size_of::<Self>()
            + self.directory.capacity()
            + self.cache.metadata_bytes()
            + self.cells.capacity() * size_of::<Cell>()
            + self.chunks.capacity() * size_of::<FrameChunk>()
            + self
                .chunks
                .iter()
                .map(|c| c.file.capacity() + c.hash.capacity())
                .sum::<usize>()
            + self.encoded.capacity() * size_of::<Option<Encoded>>()
            + self.inflight.len() * size_of::<u16>()
            + completed.capacity() * size_of::<(u16, Result<Vec<u8>, JsValue>)>()
            + art.resident.capacity() * size_of::<Option<AtlasAddress>>()
            + art.actor_rows.capacity() * size_of::<usize>()
            + art
                .actors
                .iter()
                .filter(|frames| frames.iter().any(|f| f.stream != 0))
                .map(|frames| frames.capacity() * size_of::<GameFrame>())
                .sum::<usize>();
        [bytes, self.inflight_bytes(), metadata, self.cache.len()]
    }
    fn cell(&self, id: u32) -> Result<&Cell, JsValue> {
        self.cells
            .get(id.checked_sub(1).ok_or("Invalid native frame id")? as usize)
            .ok_or_else(|| error("Invalid native frame id"))
    }
    fn cached_bytes(&self) -> usize {
        self.encoded.iter().flatten().map(|e| e.bytes.len()).sum()
    }
    fn inflight_bytes(&self) -> usize {
        self.inflight
            .iter()
            .map(|&id| self.chunks[id as usize].bytes as usize)
            .sum()
    }
    pub(crate) fn requests(&mut self, required: &[u32]) -> Result<Vec<u16>, JsValue> {
        let mut wanted = BTreeSet::new();
        let mut order = Vec::new();
        for &id in required {
            let chunk = self.cell(id)?.location.chunk;
            if wanted.insert(chunk) {
                order.push(chunk);
            }
        }
        let mut requests = Vec::new();
        for &id in &order {
            if self.encoded[id as usize].is_some() || self.inflight.contains(&id) {
                continue;
            }
            let bytes = self.chunks[id as usize].bytes as usize;
            if self.inflight.len() >= MAX_REQUESTS || self.inflight_bytes() + bytes > INFLIGHT_BYTES
            {
                break;
            }
            while self.cached_bytes() + self.inflight_bytes() + bytes > ENCODED_BYTES {
                let victim = self
                    .encoded
                    .iter()
                    .enumerate()
                    .filter_map(|(id, e)| e.as_ref().map(|e| (id, e.touched)))
                    .min_by_key(|&(id, touched)| (touched, id))
                    .map(|(id, _)| id);
                let Some(victim) = victim else {
                    break;
                };
                self.encoded[victim] = None;
            }
            if self.cached_bytes() + self.inflight_bytes() + bytes > ENCODED_BYTES {
                continue;
            }
            self.inflight.insert(id);
            requests.push(id);
        }
        Ok(requests)
    }
    pub(crate) fn accept(&mut self, id: u16, bytes: Vec<u8>) -> Result<(), JsValue> {
        if !self.inflight.contains(&id) {
            return Err(error("Unexpected native chunk response"));
        }
        self.chunks[id as usize]
            .verify_payload(&bytes)
            .map_err(error)?;
        self.inflight.remove(&id);
        self.clock += 1;
        self.encoded[id as usize] = Some(Encoded {
            bytes,
            touched: self.clock,
        });
        Ok(())
    }
    fn decode(&mut self, id: u32) -> Result<Option<[Vec<u8>; 3]>, JsValue> {
        let cell = *self.cell(id)?;
        self.clock += 1;
        let Some(chunk) = self.encoded[cell.location.chunk as usize].as_mut() else {
            return Ok(None);
        };
        chunk.touched = self.clock;
        compressed::decode_frame(cell.location.slice(&chunk.bytes).map_err(error)?, cell.size)
            .map(Some)
            .map_err(error)
    }
}

#[wasm_bindgen(module = "/src/game_assets/native/fetch.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn fetch_chunk(name: &str, expected: u32) -> Result<JsValue, JsValue>;
}

#[cfg(test)]
mod tests;
