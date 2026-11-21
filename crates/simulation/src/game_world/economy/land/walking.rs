//! Walk-cost fields shared by the offices of one player. Offices in different
//! branches divide work independently: e.g. a Steward can gather under a
//! Vassal's protection.
use crate::{GameWorld, Terrain};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::{ObjectKind, ResourceNode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use super::super::gathering::BLOCK_TILES;
use super::super::steward::STEWARD_RULES;

const LOCALITY: i32 = 64;
const RADIUS: i32 = STEWARD_RULES.radius_tiles;
const ORTHOGONAL: u16 = 10;
const DIAGONAL: u16 = 14;
const REACH: usize = RADIUS as usize * ORTHOGONAL as usize;
const NEIGHBORS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

fn neighbors(at: TileCoord) -> impl Iterator<Item = TileCoord> {
    NEIGHBORS
        .into_iter()
        .map(move |(dx, dy)| TileCoord::new(at.x + dx, at.y + dy))
}

fn pages(rect: TileRect, side: i32) -> impl Iterator<Item = (i32, i32)> {
    let lo = (rect.min.x.div_euclid(side), rect.min.y.div_euclid(side));
    let hi = (
        (rect.max.x - 1).div_euclid(side),
        (rect.max.y - 1).div_euclid(side),
    );
    (lo.1..=hi.1).flat_map(move |y| (lo.0..=hi.0).map(move |x| (x, y)))
}

fn page(tile: TileCoord, side: i32) -> (i32, i32) {
    (tile.x.div_euclid(side), tile.y.div_euclid(side))
}

#[derive(Debug)]
struct Field {
    origin: TileRect,
    center: TileCoord,
    bounds: TileRect,
    revision: u64,
    // Uniform terrain has a closed form and needs no raster.
    costs: Option<Vec<u16>>,
}

impl Field {
    fn new(world: &GameWorld, origin: TileRect) -> Self {
        let center = TileCoord::new(
            (origin.min.x + origin.max.x) / 2,
            (origin.min.y + origin.max.y) / 2,
        );
        let bounds = TileRect::from_xywh(
            center.x - RADIUS,
            center.y - RADIUS,
            RADIUS * 2 + 1,
            RADIUS * 2 + 1,
        )
        .clamp(world.config.width_tiles, world.config.height_tiles);
        let mut field = Self {
            origin,
            center,
            bounds,
            revision: world.economy.walking.latest(bounds),
            costs: None,
        };
        if matches!(world.terrain, Terrain::Uniform(_)) {
            return field;
        }
        let mut costs = vec![u16::MAX; (bounds.width() * bounds.height()) as usize];
        let mut wave = vec![Vec::new(); REACH + 1];
        for seed in super::super::placement::perimeter(origin) {
            if let Some(cost) = field.cost(seed)
                && world.terrain.passable(seed, world.config)
            {
                costs[field.index(seed)] = cost;
                wave[usize::from(cost)].push(seed);
            }
        }
        // The office itself belongs to its source, even though routes begin
        // at the actual exits of its footprint.
        for y in origin.min.y..origin.max.y {
            for x in origin.min.x..origin.max.x {
                let tile = TileCoord::new(x, y);
                if let Some(cost) = field.cost(tile) {
                    costs[field.index(tile)] = cost;
                }
            }
        }
        for d in 0..=REACH {
            for from in std::mem::take(&mut wave[d]) {
                if usize::from(costs[field.index(from)]) != d {
                    continue;
                }
                for to in neighbors(from).filter(|&to| bounds.contains(to)) {
                    let step = if from.x == to.x || from.y == to.y {
                        ORTHOGONAL
                    } else {
                        DIAGONAL
                    };
                    let nd = d + usize::from(step);
                    let i = field.index(to);
                    if nd <= REACH
                        && nd < usize::from(costs[i])
                        && world.terrain.ground_step_open(from, to)
                    {
                        costs[i] = nd as u16;
                        wave[nd].push(to);
                    }
                }
            }
        }
        field.costs = Some(costs);
        field
    }

    fn index(&self, tile: TileCoord) -> usize {
        ((tile.y - self.bounds.min.y) * self.bounds.width() + tile.x - self.bounds.min.x) as usize
    }

    fn cost(&self, tile: TileCoord) -> Option<u16> {
        if !self.bounds.contains(tile) {
            return None;
        }
        if let Some(costs) = &self.costs {
            let cost = costs[self.index(tile)];
            return (cost != u16::MAX).then_some(cost);
        }
        let dx = tile.x.abs_diff(self.center.x);
        let dy = tile.y.abs_diff(self.center.y);
        let cost = dx.abs_diff(dy) * u32::from(ORTHOGONAL) + dx.min(dy) * u32::from(DIAGONAL);
        (cost <= REACH as u32).then_some(cost as u16)
    }

    fn approach_cost(&self, tile: TileCoord) -> Option<u16> {
        self.cost(tile)
            .or_else(|| neighbors(tile).filter_map(|t| self.cost(t)).min())
    }
}

#[derive(Debug)]
pub(in super::super) struct Walking {
    sources: Vec<(EntityId, TileRect)>,
    fields: Vec<Arc<Field>>,
    nearby: BTreeMap<(i32, i32), Vec<usize>>,
    patch_owners: BTreeMap<TileCoord, EntityId>,
    patch_pages: BTreeSet<(i32, i32)>,
    revision: u64,
}

impl Walking {
    fn new(
        world: &GameWorld,
        sources: Vec<(EntityId, TileRect)>,
        previous: Option<&Arc<Self>>,
    ) -> Self {
        let cache = &world.economy.walking;
        let fields: Vec<_> = sources
            .iter()
            .map(|&(id, origin)| {
                previous
                    .and_then(|old| {
                        let index = old
                            .sources
                            .binary_search_by_key(&id, |&(source, _)| source)
                            .ok()?;
                        let field = &old.fields[index];
                        (field.origin == origin && field.revision == cache.latest(field.bounds))
                            .then(|| field.clone())
                    })
                    .unwrap_or_else(|| Arc::new(Field::new(world, origin)))
            })
            .collect();
        let mut nearby = BTreeMap::<_, Vec<_>>::new();
        for (i, field) in fields.iter().enumerate() {
            let r = field.bounds;
            let expanded =
                TileRect::from_xywh(r.min.x - 1, r.min.y - 1, r.width() + 2, r.height() + 2);
            for cell in pages(expanded, LOCALITY) {
                nearby.entry(cell).or_default().push(i);
            }
        }
        let mut walking = Self {
            sources,
            fields,
            nearby,
            patch_owners: BTreeMap::new(),
            patch_pages: BTreeSet::new(),
            revision: cache.revision,
        };
        walking.assign_patches(world);
        walking
    }

    fn matches(&self, sources: &[(EntityId, TileRect)], cache: &ZoneCache) -> bool {
        self.sources == sources
            && (self.revision == cache.revision
                || (self
                    .fields
                    .iter()
                    .all(|field| field.revision == cache.latest(field.bounds))
                    && self
                        .patch_pages
                        .iter()
                        .all(|page| cache.page_revision(*page) <= self.revision)))
    }

    fn candidates(&self, at: TileCoord) -> impl Iterator<Item = (EntityId, &Field)> {
        self.nearby
            .get(&page(at, LOCALITY))
            .into_iter()
            .flatten()
            .map(|&i| (self.sources[i].0, self.fields[i].as_ref()))
    }

    pub(in super::super) fn owns(&self, id: EntityId, at: TileCoord) -> bool {
        self.owner(at) == Some(id)
    }

    pub(in super::super) fn patch_owner(&self, at: TileCoord) -> Option<EntityId> {
        self.patch_owners.get(&at).copied()
    }

    fn owner(&self, at: TileCoord) -> Option<EntityId> {
        self.patch_owner(at).or_else(|| {
            self.candidates(at)
                .filter_map(|(id, field)| Some((field.cost(at)?, id)))
                .min()
                .map(|(_, id)| id)
        })
    }

    fn assign_patches(&mut self, world: &GameWorld) {
        let Terrain::Map { generator, overlay } = &world.terrain else {
            return;
        };
        let chunks: BTreeSet<_> = self
            .fields
            .iter()
            .flat_map(|field| pages(field.bounds, BLOCK_TILES))
            .collect();
        let land_node = |n: &ResourceNode| {
            !matches!(n.object, ObjectKind::ShoreFish | ObjectKind::DeepFish)
                && overlay.resource_available(*n)
        };
        let mut visited = BTreeSet::new();
        let mut blocks = BTreeMap::new();
        for chunk in chunks {
            let nodes = blocks
                .entry(chunk)
                .or_insert_with(|| world.block_resource_nodes(generator, overlay, chunk))
                .clone();
            for node in nodes.iter().filter(|n| land_node(n)) {
                if !visited.insert(node.tile) {
                    continue;
                }
                let mut tiles = Vec::new();
                let mut queue = VecDeque::from([node.tile]);
                let mut best = None;
                while let Some(tile) = queue.pop_front() {
                    tiles.push(tile);
                    self.patch_pages.insert(page(tile, LOCALITY));
                    for (id, field) in self.candidates(tile) {
                        if let Some(cost) = field.approach_cost(tile) {
                            let rank = (cost, id);
                            best =
                                Some(best.map_or(rank, |prior: (u16, EntityId)| prior.min(rank)));
                        }
                    }
                    for neighbor in neighbors(tile) {
                        if visited.contains(&neighbor)
                            || neighbor.x < 0
                            || neighbor.y < 0
                            || neighbor.x >= world.config.width_tiles
                            || neighbor.y >= world.config.height_tiles
                        {
                            continue;
                        }
                        let block = (
                            neighbor.x.div_euclid(BLOCK_TILES),
                            neighbor.y.div_euclid(BLOCK_TILES),
                        );
                        if blocks
                            .entry(block)
                            .or_insert_with(|| {
                                world.block_resource_nodes(generator, overlay, block)
                            })
                            .iter()
                            .any(|n| n.tile == neighbor && n.kind == node.kind && land_node(n))
                            && visited.insert(neighbor)
                        {
                            queue.push_back(neighbor);
                        }
                    }
                }
                if let Some((_, owner)) = best {
                    self.patch_owners
                        .extend(tiles.into_iter().map(|t| (t, owner)));
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ZoneCache {
    revision: u64,
    reset: u64,
    changed: BTreeMap<(i32, i32), u64>,
    pub(in super::super) stewards: BTreeMap<PlayerId, Arc<Walking>>,
    pub(in super::super) vassals: BTreeMap<PlayerId, Arc<Walking>>,
}

impl ZoneCache {
    fn page_revision(&self, page: (i32, i32)) -> u64 {
        self.changed.get(&page).copied().unwrap_or(self.reset)
    }

    fn latest(&self, rect: TileRect) -> u64 {
        pages(rect, LOCALITY)
            .map(|page| self.page_revision(page))
            .max()
            .unwrap_or(self.reset)
    }
}

impl GameWorld {
    pub(crate) fn management_ground_changed(&mut self, area: Option<TileRect>) {
        let cache = &mut self.economy.walking;
        if cache.stewards.is_empty() && cache.vassals.is_empty() {
            return;
        }
        cache.revision += 1;
        if let Some(area) = area {
            for page in pages(area, LOCALITY) {
                cache.changed.insert(page, cache.revision);
            }
        } else {
            cache.reset = cache.revision;
            cache.changed.clear();
        }
    }

    pub(in super::super) fn walking_group(
        &self,
        sources: Vec<(EntityId, TileRect)>,
        previous: Option<&Arc<Walking>>,
    ) -> Arc<Walking> {
        if let Some(previous) =
            previous.filter(|previous| previous.matches(&sources, &self.economy.walking))
        {
            return previous.clone();
        }
        Arc::new(Walking::new(self, sources, previous))
    }
}
