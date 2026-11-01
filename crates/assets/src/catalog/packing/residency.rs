//! Bounded native-frame residency in paired colour and owner-mask pages.
use super::{MAX_FRAME_EXTENT, MAX_LOGICAL_PAGES, PAGE_SIDE, Placement};

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
    limit: u16,
    occupied: Vec<[u32; GRID]>,
    entries: Vec<(u64, Entry)>,
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
            limit,
            occupied: vec![[0; GRID]; usize::from(pairs)],
            entries: Vec::new(),
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
    /// Owned allocations; allocator bookkeeping is excluded.
    pub fn metadata_bytes(&self) -> usize {
        self.occupied.capacity() * std::mem::size_of::<[u32; GRID]>()
            + self.entries.capacity() * std::mem::size_of::<(u64, Entry)>()
    }
    pub fn lookup(&self, key: u64) -> Option<Placement> {
        self.index(key)
            .ok()
            .map(|index| self.entries[index].1.placement)
    }
    // Residency is physically bounded by atlas tiles. A sorted contiguous
    // inventory keeps lookups logarithmic without per-node allocations.
    fn index(&self, key: u64) -> Result<usize, usize> {
        self.entries.binary_search_by_key(&key, |(key, _)| *key)
    }
    /// Protect current demand before admission. A cell remains pinned through
    /// three frames, including the last frame in which it was required.
    pub fn begin_frame(&mut self, required: &[u64]) {
        self.frame = self.frame.wrapping_add(1);
        if self.frame == 0 {
            self.frame = 1;
            for (_, entry) in &mut self.entries {
                entry.touched = 0;
                entry.protected = 0;
            }
        }
        self.protect(required);
    }
    /// Mark the cells used by a successfully presented frame without advancing
    /// the turn. This includes animation fallbacks admitted during that turn.
    pub fn protect(&mut self, used: &[u64]) {
        for key in used {
            if let Ok(index) = self.index(*key) {
                let entry = &mut self.entries[index].1;
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
        allow_growth: bool,
    ) -> Result<Reservation, &'static str> {
        if size
            .into_iter()
            .any(|side| side == 0 || side > MAX_FRAME_EXTENT)
        {
            return Err("invalid native frame dimensions");
        }
        if let Ok(index) = self.index(key) {
            let entry = &mut self.entries[index].1;
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
                let index = self.index(key).expect_err("new native key");
                self.entries.insert(
                    index,
                    (
                        key,
                        Entry {
                            placement,
                            touched: self.frame,
                            protected: if required { self.frame } else { 0 },
                        },
                    ),
                );
                return Ok(Reservation {
                    placement: Some(placement),
                    evicted,
                });
            }
            let victim = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, (_, entry))| {
                    entry.protected == 0 || self.frame.saturating_sub(entry.protected) >= 3
                })
                .min_by_key(|(_, (key, entry))| (entry.touched, *key))
                .map(|(index, _)| index);
            let Some(victim) = victim else {
                let pairs = self.occupied.len();
                let maximum = usize::from((self.limit - self.base) / 2);
                if required && allow_growth && pairs < maximum {
                    let next = (pairs * 2).min(maximum);
                    self.occupied.resize(next, [0; GRID]);
                    self.layers = self.base + next as u16 * 2;
                    continue;
                }
                return Ok(Reservation {
                    placement: None,
                    evicted,
                });
            };
            let (key, entry) = self.entries.remove(victim);
            self.mark(entry.placement, false);
            evicted.push(key);
        }
    }
}
#[path = "residency/tests.rs"]
#[cfg(test)]
mod tests;
