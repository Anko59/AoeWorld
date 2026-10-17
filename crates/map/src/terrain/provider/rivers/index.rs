//! Page-local broad phase. The exact integer corridor/station query remains
//! authoritative; bins only remove segments whose expanded box cannot hit.
use crate::{EnvironmentPage, EnvironmentPageKey, RiverSegment};
use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};
#[path = "curves.rs"]
mod curves;

const BINS: i64 = 16;
const MAX_INDEX_PAGES: usize = 64;
const WATER_ROWS: i32 = 32;
const WATER_COLUMNS: i32 = 512;
type WaterEntry = Option<(aoe_core::TileCoord, crate::WaterKind)>;

#[derive(Debug, Default)]
pub(in crate::terrain) struct Cache {
    pages: VecDeque<(EnvironmentPageKey, Arc<Index>)>,
    water: Option<Box<[WaterEntry]>>,
    validated: VecDeque<(EnvironmentPageKey, Weak<EnvironmentPage>)>,
}

impl Cache {
    pub(in crate::terrain) fn validated(
        &self,
        key: EnvironmentPageKey,
        page: &Arc<EnvironmentPage>,
    ) -> bool {
        self.validated
            .iter()
            .rev()
            .any(|(k, handle)| *k == key && handle.ptr_eq(&Arc::downgrade(page)))
    }
    pub(in crate::terrain) fn remember_validated(
        &mut self,
        key: EnvironmentPageKey,
        page: &Arc<EnvironmentPage>,
    ) {
        self.validated.retain(|(k, _)| *k != key);
        self.validated.push_back((key, Arc::downgrade(page)));
        if self.validated.len() > MAX_INDEX_PAGES {
            self.validated.pop_front();
        }
    }

    fn water_slot(tile: aoe_core::TileCoord) -> usize {
        (tile.y.rem_euclid(WATER_ROWS) * WATER_COLUMNS + tile.x.rem_euclid(WATER_COLUMNS)) as usize
    }
    pub(in crate::terrain) fn water(&self, tile: aoe_core::TileCoord) -> Option<crate::WaterKind> {
        self.water.as_ref()?[Self::water_slot(tile)]
            .filter(|(key, _)| *key == tile)
            .map(|(_, kind)| kind)
    }
    pub(in crate::terrain) fn insert_water(
        &mut self,
        tile: aoe_core::TileCoord,
        kind: crate::WaterKind,
    ) {
        let water = self.water.get_or_insert_with(|| {
            vec![None; (WATER_ROWS * WATER_COLUMNS) as usize].into_boxed_slice()
        });
        water[Self::water_slot(tile)] = Some((tile, kind));
    }

    pub(super) fn get(&mut self, key: EnvironmentPageKey) -> Option<Arc<Index>> {
        let position = self.pages.iter().rposition(|(k, _)| *k == key)?;
        let entry = self.pages.remove(position)?;
        let index = entry.1.clone();
        self.pages.push_back(entry);
        Some(index)
    }
    pub(super) fn insert(&mut self, key: EnvironmentPageKey, index: Arc<Index>) {
        self.pages.retain(|(k, _)| *k != key);
        self.pages.push_back((key, index));
        while self.pages.len() > MAX_INDEX_PAGES {
            self.pages.pop_front();
        }
    }
}

#[derive(Debug)]
pub(super) struct Index {
    origin: [i64; 2],
    extent: i64,
    segments: Vec<RiverSegment>,
    shaped: Vec<Vec<RiverSegment>>,
    bins: Vec<Vec<u16>>,
}

impl Index {
    pub(super) fn new(segments: &[RiverSegment], origin: [i64; 2], extent: i64) -> Self {
        let mut index = Self {
            origin,
            extent: extent.max(1),
            segments: segments.to_vec(),
            shaped: curves::shape(segments),
            bins: vec![Vec::new(); (BINS * BINS) as usize],
        };
        for (id, segment) in segments.iter().enumerate() {
            let radius = i64::from(segment.width_units) / 2 + 3 * 256;
            let low = [0, 1].map(|axis| {
                index.bin(
                    i64::from(segment.start[axis].min(segment.end[axis])) - radius,
                    axis,
                )
            });
            let high = [0, 1].map(|axis| {
                index.bin(
                    i64::from(segment.start[axis].max(segment.end[axis])) + radius,
                    axis,
                )
            });
            for y in low[1]..=high[1] {
                for x in low[0]..=high[0] {
                    index.bins[(y * BINS + x) as usize].push(id as u16);
                }
            }
        }
        index
    }
    fn bin(&self, value: i64, axis: usize) -> i64 {
        ((value - self.origin[axis]).div_euclid((self.extent + BINS - 1) / BINS)).clamp(0, BINS - 1)
    }
    pub(super) fn possible(&self, point: [i64; 2], radius: i64) -> bool {
        let low = [0, 1].map(|axis| self.bin(point[axis] - radius, axis));
        let high = [0, 1].map(|axis| self.bin(point[axis] + radius, axis));
        (low[1]..=high[1])
            .any(|y| (low[0]..=high[0]).any(|x| !self.bins[(y * BINS + x) as usize].is_empty()))
    }
    pub(super) fn sample(
        &self,
        point: [i64; 2],
        radius: i64,
        min_width_units: u16,
    ) -> Option<(i128, bool)> {
        self.query(point, radius, None, min_width_units)
    }
    pub(super) fn physical(
        &self,
        point: [i64; 2],
        radius: i64,
        width_noise: i64,
    ) -> Option<(i128, bool)> {
        self.query(point, radius, Some(width_noise), 0)
    }
    fn query(
        &self,
        point: [i64; 2],
        radius: i64,
        width_noise: Option<i64>,
        min_width_units: u16,
    ) -> Option<(i128, bool)> {
        let radius = radius.max(0);
        let low = [0, 1].map(|axis| self.bin(point[axis] - radius, axis));
        let high = [0, 1].map(|axis| self.bin(point[axis] + radius, axis));
        let mut best = None;
        // A segment may occupy several queried bins. Preserve source order
        // for equal distances (and therefore ford ties), testing it once.
        let mut seen = [0_u64; crate::MAX_RIVER_SEGMENTS_PER_PAGE.div_ceil(64)];
        for y in low[1]..=high[1] {
            for x in low[0]..=high[0] {
                for &id in &self.bins[(y * BINS + x) as usize] {
                    seen[usize::from(id) / 64] |= 1 << (id % 64);
                }
            }
        }
        for (word_index, mut word) in seen.into_iter().enumerate() {
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let id = word_index * 64 + bit;
                if self.segments[id].width_units < min_width_units {
                    continue;
                }
                let parts = if width_noise.is_some() {
                    &self.shaped[id][..]
                } else {
                    std::slice::from_ref(&self.segments[id])
                };
                for part in parts {
                    let mut part = *part;
                    if let Some(noise) = width_noise {
                        part.width_units = (i64::from(part.width_units) + 2 * noise).clamp(
                            i64::from(crate::MIN_RIVER_WIDTH_UNITS),
                            i64::from(crate::MAX_RIVER_WIDTH_UNITS),
                        ) as u16;
                    }
                    if let Some(hit) = part.sample(point, radius)
                        && best.is_none_or(|(distance, _)| hit.0 < distance)
                    {
                        best = Some(hit);
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
#[path = "../../tests/rivers/index.rs"]
mod tests;
