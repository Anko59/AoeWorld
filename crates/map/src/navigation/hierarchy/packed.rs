//! Fixed-width, endian-stable navigation views. Validation happens once at
//! installation; queries read records directly, without decoding or caching.
use super::*;
use std::ops::Range;
mod codec;
pub(super) use codec::{decode, encode};

pub trait NavigationStorage: std::fmt::Debug + Send + Sync {
    fn bytes(&self) -> &[u8];
    /// Owned heap only; native file mappings report zero here.
    fn heap_bytes(&self) -> usize;
}
impl NavigationStorage for Vec<u8> {
    fn bytes(&self) -> &[u8] {
        self
    }
    fn heap_bytes(&self) -> usize {
        self.capacity()
    }
}

const HEADER: usize = 20;
const RUN: usize = 6;
const NODE: usize = 16;
const PORTAL: usize = 8;
const MAX_BYTES: usize = 4 << 20;

#[derive(Clone, Debug)]
pub(super) struct PackedChunk {
    pub(super) storage: Arc<dyn NavigationStorage>,
    pub(super) range: Range<usize>,
}

#[derive(Clone, Copy)]
pub(super) struct View<'a> {
    bytes: &'a [u8],
    runs: usize,
    nodes: usize,
    portals: usize,
    costs: usize,
    labels: usize,
    links: usize,
    end: usize,
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl PackedChunk {
    pub(super) fn view(&self) -> View<'_> {
        View::new(&self.storage.bytes()[self.range.clone()])
    }
}

impl<'a> View<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        let runs = HEADER;
        let nodes = runs + usize::from(u16_at(bytes, 8)) * RUN;
        let portals = nodes + usize::from(u16_at(bytes, 10)) * NODE;
        let n = usize::from(u16_at(bytes, 12));
        let costs = portals + n * PORTAL;
        let labels = costs + n * n * 4;
        let links = labels + usize::from(u16_at(bytes, 14)) * 4;
        let end = links + usize::from(u16_at(bytes, 16)) * 4;
        Self {
            bytes,
            runs,
            nodes,
            portals,
            costs,
            labels,
            links,
            end,
        }
    }

    pub(super) fn key(self) -> Key {
        (u32_at(self.bytes, 0) as i32, u32_at(self.bytes, 4) as i32)
    }
    pub(super) fn portal_count(self) -> usize {
        usize::from(u16_at(self.bytes, 12))
    }
    pub(super) fn components(self) -> u16 {
        u16_at(self.bytes, 18)
    }
    pub(super) fn link_count(self) -> usize {
        usize::from(u16_at(self.bytes, 16))
    }
    pub(super) fn link(self, index: usize) -> (u16, u16) {
        let at = self.links + index * 4;
        (u16_at(self.bytes, at), u16_at(self.bytes, at + 2))
    }

    fn run_at(self, start: usize, stride: usize, count: usize, tile: usize) -> usize {
        let (mut lo, mut hi) = (0, count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if usize::from(u16_at(self.bytes, start + mid * stride)) <= tile {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        start + lo * stride
    }

    pub(super) fn label(self, tile: usize) -> u16 {
        let at = self.run_at(self.labels, 4, usize::from(u16_at(self.bytes, 14)), tile);
        u16_at(self.bytes, at + 2)
    }

    pub(super) fn links(self) -> impl Iterator<Item = (u16, u16)> + 'a {
        (self.links..self.end)
            .step_by(4)
            .map(move |at| (u16_at(self.bytes, at), u16_at(self.bytes, at + 2)))
    }

    pub(super) fn portal(self, index: usize) -> Portal {
        let at = self.portals + index * PORTAL;
        let key = self.key();
        Portal {
            tile: grid::tile(key, usize::from(u16_at(self.bytes, at))),
            cross: TileCoord::new(
                key.0 * CHUNK_TILES + i32::from(self.bytes[at + 2] as i8),
                key.1 * CHUNK_TILES + i32::from(self.bytes[at + 3] as i8),
            ),
            crossing_cost: u32::from(u16_at(self.bytes, at + 4)),
            bidirectional: self.bytes[at + 6] != 0,
        }
    }

    pub(super) fn cost(self, from: usize, to: usize) -> Option<u32> {
        let cost = u32_at(
            self.bytes,
            self.costs + (from * self.portal_count() + to) * 4,
        );
        (cost != u32::MAX).then_some(cost)
    }

    pub(super) fn cell(self, tile: usize) -> Option<WalkCell> {
        let at = self.run_at(self.runs, RUN, usize::from(u16_at(self.bytes, 8)), tile);
        let flags = self.bytes[at + 4];
        if flags & 1 == 0 {
            return None;
        }
        let count = usize::from(u16_at(self.bytes, 10));
        let (mut lo, mut hi) = (0, count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if usize::from(u16_at(self.bytes, self.nodes + mid * NODE + 8)) < tile {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let node = (lo < count
            && usize::from(u16_at(self.bytes, self.nodes + lo * NODE + 8)) == tile)
            .then(|| self.node(lo));
        Some(WalkCell {
            passable: flags & 2 != 0,
            walkable_surface: flags & 4 != 0,
            mud: flags & 8 != 0,
            game_height_level: u16_at(self.bytes, at + 2) as i16,
            node,
        })
    }

    pub(super) fn node(self, index: usize) -> crate::ResourceNode {
        let at = self.nodes + index * NODE;
        use crate::{ObjectKind::*, ResourceKind::*};
        crate::ResourceNode {
            id: u64::from_le_bytes([
                self.bytes[at],
                self.bytes[at + 1],
                self.bytes[at + 2],
                self.bytes[at + 3],
                self.bytes[at + 4],
                self.bytes[at + 5],
                self.bytes[at + 6],
                self.bytes[at + 7],
            ]),
            tile: grid::tile(self.key(), usize::from(u16_at(self.bytes, at + 8))),
            initial_amount: u16_at(self.bytes, at + 10),
            kind: [Food, Wood, Gold, Stone][usize::from(self.bytes[at + 12])],
            object: [
                Tree,
                ForageBush,
                GoldDeposit,
                StoneDeposit,
                Decoration,
                ShoreFish,
                DeepFish,
            ][usize::from(self.bytes[at + 13])],
            visual_variant: self.bytes[at + 14],
        }
    }

    pub(super) fn validate(bytes: &'a [u8]) -> Result<Self> {
        let invalid = EnvironmentPageError::Invalid;
        if bytes.len() < HEADER || bytes.len() > MAX_BYTES {
            return Err(invalid);
        }
        if usize::from(u16_at(bytes, 12)) > grid::CELLS * 8 {
            return Err(invalid);
        }
        let view = Self::new(bytes);
        let maximum = crate::request::MAX_TILES_PER_SIDE.div_ceil(CHUNK_TILES as u64) as i32;
        let (x, y) = view.key();
        if view.end != bytes.len()
            || x < 0
            || y < 0
            || x >= maximum
            || y >= maximum
            || u16_at(bytes, 8) as usize > grid::CELLS
            || u16_at(bytes, 10) as usize > grid::CELLS
            || view.portal_count() > grid::CELLS * 8
            || view.components() as usize > grid::CELLS
        {
            return Err(invalid);
        }
        for (start, end, stride, labels) in [
            (view.runs, view.nodes, RUN, false),
            (view.labels, view.links, 4, true),
        ] {
            let mut last = 0;
            for at in (start..end).step_by(stride) {
                let next = u16_at(bytes, at);
                if next <= last
                    || next as usize > grid::CELLS
                    || (labels && u16_at(bytes, at + 2) > view.components())
                    || (!labels && (bytes[at + 4] > 15 || bytes[at + 5] != 0))
                {
                    return Err(invalid);
                }
                last = next;
            }
            if last as usize != grid::CELLS {
                return Err(invalid);
            }
        }
        let mut last = None;
        for at in (view.nodes..view.portals).step_by(NODE) {
            let tile = u16_at(bytes, at + 8);
            if tile as usize >= grid::CELLS
                || last.is_some_and(|last| last >= tile)
                || bytes[at + 12] > 3
                || bytes[at + 13] > 6
                || bytes[at + 15] != 0
            {
                return Err(invalid);
            }
            last = Some(tile);
        }
        for index in 0..view.portal_count() {
            let at = view.portals + index * PORTAL;
            if u16_at(bytes, at) as usize >= grid::CELLS || bytes[at + 6] > 1 || bytes[at + 7] != 0
            {
                return Err(invalid);
            }
            let portal = view.portal(index);
            if (portal.tile.x - portal.cross.x).abs() > 1
                || (portal.tile.y - portal.cross.y).abs() > 1
                || inside(view.key(), portal.cross)
            {
                return Err(invalid);
            }
        }
        if view
            .links()
            .any(|(a, b)| a == 0 || b == 0 || a > view.components() || b > view.components())
        {
            return Err(invalid);
        }
        Ok(view)
    }
}
