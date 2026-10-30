//! Bounded native-frame residency in paired colour and owner-mask pages.
use super::{MAX_FRAME_EXTENT, MAX_LOGICAL_PAGES, PAGE_SIDE, Placement};
use std::collections::BTreeMap;

/// Whole tiles have 256-byte RGBA rows for aligned GPU subrectangle uploads.
pub const TILE_SIDE: u16 = 64;
pub const GUTTER: u16 = 2;
const GRID: usize = (PAGE_SIDE / TILE_SIDE) as usize;

pub struct Reservation {
    pub placement: Option<Placement>,
    /// Every removed key is returned, even when protected cells prevent a fit.
    pub evicted: Vec<u64>,
}
struct Entry {
    placement: Placement,
    touched: u64,
    protected: u64,
}
pub struct FrameCache {
    base: u16,
    layers: u16,
    occupied: Vec<[u32; GRID]>,
    entries: BTreeMap<u64, Entry>,
    frame: u64,
}

fn span(side: u16) -> usize {
    usize::from((side + 2 * GUTTER).div_ceil(TILE_SIDE))
}
fn mask(width: usize, x: usize) -> u32 {
    if width == GRID {
        u32::MAX
    } else {
        ((1_u32 << width) - 1) << x
    }
}

impl FrameCache {
    /// The renderer supplies its existing physical layer limit. Additional
    /// colour/mask pairs begin at an even layer after the eight base layers.
    pub fn new(base: u16, pairs: u16, limit: u16) -> Result<Self, &'static str> {
        let layers = pairs
            .checked_mul(2)
            .and_then(|n| base.checked_add(n))
            .ok_or("invalid native cache budget")?;
        if base < 8
            || !base.is_multiple_of(2)
            || pairs == 0
            || layers > limit
            || usize::from(limit) > MAX_LOGICAL_PAGES
        {
            return Err("invalid native cache budget");
        }
        Ok(Self {
            base,
            layers,
            occupied: vec![[0; GRID]; usize::from(pairs)],
            entries: BTreeMap::new(),
            frame: 1,
        })
    }
    pub fn layer_count(&self) -> u16 {
        self.layers
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Owned payload; allocator and BTreeMap node bookkeeping are excluded.
    pub fn metadata_bytes(&self) -> usize {
        self.occupied.capacity() * std::mem::size_of::<[u32; GRID]>()
            + self.entries.len() * std::mem::size_of::<(u64, Entry)>()
    }
    pub fn lookup(&self, key: u64) -> Option<Placement> {
        self.entries.get(&key).map(|entry| entry.placement)
    }
    /// Protect all already resident cells needed by this frame before admitting
    /// misses. Newly admitted current cells must also pass `required = true`.
    pub fn begin_frame(&mut self, required: &[u64]) {
        self.frame = self.frame.wrapping_add(1);
        if self.frame == 0 {
            self.frame = 1;
            for entry in self.entries.values_mut() {
                entry.touched = 0;
                entry.protected = 0;
            }
        }
        for key in required {
            if let Some(entry) = self.entries.get_mut(key) {
                entry.protected = self.frame;
                entry.touched = self.frame;
            }
        }
    }
    fn space(&self, size: [u16; 2]) -> Option<Placement> {
        let (width, height) = (span(size[0]), span(size[1]));
        for (page, rows) in self.occupied.iter().enumerate() {
            for y in 0..=GRID - height {
                for x in 0..=GRID - width {
                    let bits = mask(width, x);
                    if rows[y..y + height].iter().all(|row| row & bits == 0) {
                        return Some(Placement {
                            page: self.base + page as u16 * 2,
                            x: x as u16 * TILE_SIDE + GUTTER,
                            y: y as u16 * TILE_SIDE + GUTTER,
                            width: size[0],
                            height: size[1],
                        });
                    }
                }
            }
        }
        None
    }
    fn mark(&mut self, placement: Placement, occupied: bool) {
        let page = usize::from((placement.page - self.base) / 2);
        let x = usize::from((placement.x - GUTTER) / TILE_SIDE);
        let y = usize::from((placement.y - GUTTER) / TILE_SIDE);
        let bits = mask(span(placement.width), x);
        for row in &mut self.occupied[page][y..y + span(placement.height)] {
            if occupied {
                *row |= bits;
            } else {
                *row &= !bits;
            }
        }
    }
    pub fn reserve(
        &mut self,
        key: u64,
        size: [u16; 2],
        required: bool,
    ) -> Result<Reservation, &'static str> {
        if size
            .into_iter()
            .any(|side| side == 0 || side > MAX_FRAME_EXTENT)
        {
            return Err("invalid native frame dimensions");
        }
        if let Some(entry) = self.entries.get_mut(&key) {
            if [entry.placement.width, entry.placement.height] != size {
                return Err("native frame dimensions changed");
            }
            entry.touched = self.frame;
            if required {
                entry.protected = self.frame;
            }
            return Ok(Reservation {
                placement: Some(entry.placement),
                evicted: Vec::new(),
            });
        }
        let mut evicted = Vec::new();
        loop {
            if let Some(placement) = self.space(size) {
                self.mark(placement, true);
                self.entries.insert(
                    key,
                    Entry {
                        placement,
                        touched: self.frame,
                        protected: if required { self.frame } else { 0 },
                    },
                );
                return Ok(Reservation {
                    placement: Some(placement),
                    evicted,
                });
            }
            let victim = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.protected != self.frame)
                .min_by_key(|(key, entry)| (entry.touched, **key))
                .map(|(key, _)| *key);
            let Some(victim) = victim else {
                return Ok(Reservation {
                    placement: None,
                    evicted,
                });
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.mark(entry.placement, false);
                evicted.push(victim);
            }
        }
    }
}
#[path = "residency/tests.rs"]
#[cfg(test)]
mod tests;
