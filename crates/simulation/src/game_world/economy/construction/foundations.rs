//! Sparse, per-owner spatial index of unfinished buildings.
use super::{EntityId, GameWorld, PlayerId, TileCoord, TileRect, distance_squared};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Cell {
    origin: TileCoord,
    side: i32,
}

impl Cell {
    fn root(config: aoe_core::WorldConfig) -> Self {
        Self {
            origin: TileCoord::new(0, 0),
            side: (config.width_tiles.max(config.height_tiles) as u32).next_power_of_two() as i32,
        }
    }

    fn rect(self) -> TileRect {
        TileRect::from_xywh(self.origin.x, self.origin.y, self.side, self.side)
    }

    fn children(self) -> impl Iterator<Item = Self> {
        // Small leaves share one bucket instead of allocating a node per tile.
        let side = self.side / 2;
        [(0, 0), (0, side), (side, 0), (side, side)]
            .into_iter()
            .filter(move |_| side >= 4)
            .map(move |(x, y)| Self {
                origin: TileCoord::new(self.origin.x + x, self.origin.y + y),
                side,
            })
    }

    fn path(self, rect: TileRect) -> impl Iterator<Item = Self> {
        let far = TileCoord::new(rect.max.x - 1, rect.max.y - 1);
        std::iter::successors(Some(self), move |cell| {
            cell.children().find(|child| {
                let area = child.rect();
                area.contains(rect.min) && area.contains(far)
            })
        })
    }
}

#[derive(Debug, Default)]
struct Node {
    sites: BTreeMap<EntityId, TileRect>,
    descendants: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Foundations {
    nodes: BTreeMap<(PlayerId, Cell), Node>,
    root: Option<Cell>,
}

impl Foundations {
    fn insert(
        &mut self,
        config: aoe_core::WorldConfig,
        player: PlayerId,
        id: EntityId,
        rect: TileRect,
    ) {
        let root = *self.root.get_or_insert_with(|| Cell::root(config));
        let mut path = root.path(rect).peekable();
        while let Some(cell) = path.next() {
            let node = self.nodes.entry((player, cell)).or_default();
            node.descendants += 1;
            if path.peek().is_none() {
                node.sites.insert(id, rect);
            }
        }
    }

    fn remove(&mut self, player: PlayerId, id: EntityId, rect: TileRect) {
        let Some(root) = self.root else { return };
        for cell in root.path(rect) {
            let key = (player, cell);
            if let Some(node) = self.nodes.get_mut(&key) {
                node.sites.remove(&id);
                node.descendants -= 1;
                if node.descendants == 0 {
                    self.nodes.remove(&key);
                }
            }
        }
    }

    pub(super) fn nearest(&self, player: PlayerId, tile: TileCoord) -> Nearest {
        let mut queue = BinaryHeap::new();
        if let Some(root) = self.root
            && self.nodes.contains_key(&(player, root))
        {
            queue.push(Reverse((0, None, root)));
        }
        Nearest {
            player,
            tile,
            queue,
        }
    }
}

pub(super) struct Nearest {
    player: PlayerId,
    tile: TileCoord,
    // At equal distances expand cells before yielding a site, then prefer
    // lower entity IDs. A cell contains each of its indexed footprints.
    queue: BinaryHeap<Reverse<(u64, Option<EntityId>, Cell)>>,
}

impl Nearest {
    pub(super) fn next(&mut self, index: &Foundations) -> Option<EntityId> {
        let distance = |rect: TileRect| {
            distance_squared(
                self.tile,
                TileCoord::new(
                    self.tile.x.clamp(rect.min.x, rect.max.x - 1),
                    self.tile.y.clamp(rect.min.y, rect.max.y - 1),
                ),
            )
        };
        while let Some(Reverse((_, site, cell))) = self.queue.pop() {
            if let Some(site) = site {
                return Some(site);
            }
            let Some(node) = index.nodes.get(&(self.player, cell)) else {
                continue;
            };
            self.queue.extend(
                node.sites
                    .iter()
                    .map(|(&id, &rect)| Reverse((distance(rect), Some(id), cell))),
            );
            self.queue.extend(
                cell.children()
                    .filter(|&child| index.nodes.contains_key(&(self.player, child)))
                    .map(|child| Reverse((distance(child.rect()), None, child))),
            );
        }
        None
    }
}

impl GameWorld {
    pub(in crate::game_world) fn unindex_foundation(
        &mut self,
        id: EntityId,
        player: PlayerId,
        rect: TileRect,
    ) {
        if rect.valid(self.config.width_tiles, self.config.height_tiles) {
            self.economy.foundations.remove(player, id, rect);
        }
    }

    pub(in crate::game_world) fn index_foundation(
        &mut self,
        id: EntityId,
        player: PlayerId,
        rect: TileRect,
    ) {
        if rect.valid(self.config.width_tiles, self.config.height_tiles) {
            self.economy
                .foundations
                .insert(self.config, player, id, rect);
        }
    }
}
