//! A bounded, exact weighted field over the shared region terrain.
use super::{NEIGHBORS, ground::chunk_index, neighbors, pages};
use crate::{GameWorld, Terrain};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::{CHUNK_TILES, MapRegionGround};
use std::collections::BTreeSet;

const RADIUS: i32 = super::super::super::steward::STEWARD_RULES.radius_tiles;
const ORTHOGONAL: u16 = 5;
const DIAGONAL: u16 = 7;
const REACH: u16 = RADIUS as u16 * ORTHOGONAL;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct TerrainKey {
    pub(super) origin: TileRect,
    pub(super) bounds: TileRect,
    bridges: u64,
    barriers: BTreeSet<TileCoord>,
}

impl TerrainKey {
    pub(super) fn new(
        world: &GameWorld,
        player: PlayerId,
        origin: TileRect,
        bounds: TileRect,
    ) -> Self {
        let start = TileCoord::new(bounds.min.x, i32::MIN);
        let end = TileCoord::new(bounds.max.x, i32::MIN);
        let barriers = world
            .economy
            .walking
            .walls
            .range(start..end)
            .filter(|&&tile| bounds.contains(tile))
            .copied()
            .chain(
                world
                    .economy
                    .gate_tiles
                    .range(start..end)
                    .filter_map(|(&tile, _)| {
                        (bounds.contains(tile) && !world.gate_allows_approach(player, tile))
                            .then_some(tile)
                    }),
            )
            .collect();
        Self {
            origin,
            bounds,
            bridges: world.economy.walking.bridge_revision(bounds),
            barriers,
        }
    }
}

#[derive(Debug)]
pub(super) struct Field {
    pub(super) terrain: TerrainKey,
    center: TileCoord,
    // Uniform terrain without barriers needs neither raster.
    ground: Option<Vec<Option<u8>>>,
    costs: Option<Vec<u16>>,
}

impl Field {
    pub(super) fn bounds(world: &GameWorld, origin: TileRect) -> TileRect {
        let center = center(origin);
        TileRect::new(
            TileCoord::new((center.x - RADIUS).max(0), (center.y - RADIUS).max(0)),
            TileCoord::new(
                (center.x + RADIUS + 1).min(world.config.width_tiles),
                (center.y + RADIUS + 1).min(world.config.height_tiles),
            ),
        )
    }

    pub(super) fn new(world: &GameWorld, terrain: TerrainKey) -> Option<Self> {
        let bounds = terrain.bounds;
        let mut field = Self {
            center: center(terrain.origin),
            terrain,
            ground: None,
            costs: None,
        };
        let area = (bounds.width() * bounds.height()) as usize;
        let mut ground;
        let overlay = match &world.terrain {
            Terrain::Uniform(_) if field.terrain.barriers.is_empty() => return Some(field),
            Terrain::Uniform(_) => {
                ground = vec![Some(u8::MAX); area];
                None
            }
            Terrain::Map { generator, overlay } => {
                ground = vec![None; area];
                let region = MapRegionGround::new(generator, overlay, &|| false);
                let size = (world.config.width_tiles, world.config.height_tiles);
                for (x, y) in pages(bounds, CHUNK_TILES) {
                    let chunk = world.economy.walking.ground_chunk(&region, (x, y), size);
                    let covered = TileRect::new(
                        TileCoord::new(
                            bounds.min.x.max(x * CHUNK_TILES),
                            bounds.min.y.max(y * CHUNK_TILES),
                        ),
                        TileCoord::new(
                            bounds.max.x.min((x + 1) * CHUNK_TILES),
                            bounds.max.y.min((y + 1) * CHUNK_TILES),
                        ),
                    );
                    for y in covered.min.y..covered.max.y {
                        let tile = TileCoord::new(covered.min.x, y);
                        let to = field.index(tile);
                        let from = chunk_index(tile);
                        let count = covered.width() as usize;
                        ground[to..to + count].copy_from_slice(&chunk[from..from + count]);
                    }
                }
                if region.error().is_some() {
                    return None;
                }
                Some(overlay)
            }
        };
        field.clip(&mut ground, overlay);
        field.costs = Some(field.walk_costs(&ground));
        field.ground = Some(ground);
        Some(field)
    }

    fn clip(&self, ground: &mut [Option<u8>], overlay: Option<&aoe_map::ResourceOverlay>) {
        let bounds = self.terrain.bounds;
        let width = bounds.width() as usize;
        let height = bounds.height() as usize;
        let strip = |cell: &mut Option<u8>, bits: u8| {
            if let Some(mask) = cell {
                *mask &= !bits;
            }
        };
        for row in ground.chunks_exact_mut(width) {
            strip(&mut row[0], 0b0010_1001);
            strip(&mut row[width - 1], 0b1001_0100);
        }
        for x in 0..width {
            strip(&mut ground[x], 0b0000_0111);
            strip(&mut ground[(height - 1) * width + x], 0b1110_0000);
        }
        for &tile in &self.terrain.barriers {
            ground[self.index(tile)] = None;
            for from in neighbors(tile).filter(|&from| bounds.contains(from)) {
                let mut bits = 0;
                for (bit, (dx, dy)) in NEIGHBORS.into_iter().enumerate() {
                    let to = TileCoord::new(from.x + dx, from.y + dy);
                    let corner = dx != 0
                        && dy != 0
                        && (tile == TileCoord::new(from.x, to.y)
                            || tile == TileCoord::new(to.x, from.y));
                    if to == tile
                        || (corner
                            && !overlay.is_some_and(|o| o.bridge_edge(from, to) == Some(true)))
                    {
                        bits |= 1 << bit;
                    }
                }
                strip(&mut ground[self.index(from)], bits);
            }
        }
    }

    fn walk_costs(&self, ground: &[Option<u8>]) -> Vec<u16> {
        let mut costs = vec![u16::MAX; ground.len()];
        let mut wave = vec![Vec::new(); usize::from(REACH) + 1];
        let bounds = self.terrain.bounds;
        let origin = self.terrain.origin;
        for seed in super::super::super::placement::perimeter(origin) {
            if bounds.contains(seed) && ground[self.index(seed)].is_some() {
                let cost = self.straight_cost(seed);
                let slot = self.index(seed);
                costs[slot] = cost;
                wave[usize::from(cost)].push(slot);
            }
        }
        // Belongs to the office even though paths start at actual exits.
        for tile in tiles(origin) {
            costs[self.index(tile)] = self.straight_cost(tile);
        }
        let width = bounds.width();
        let steps = NEIGHBORS.map(|(dx, dy)| {
            (
                dy * width + dx,
                if dx == 0 || dy == 0 {
                    ORTHOGONAL
                } else {
                    DIAGONAL
                },
            )
        });
        for distance in 0..=REACH {
            for from in std::mem::take(&mut wave[usize::from(distance)]) {
                if costs[from] != distance {
                    continue;
                }
                let mut mask = ground[from].unwrap_or(0);
                while mask != 0 {
                    let bit = mask.trailing_zeros() as usize;
                    mask &= mask - 1;
                    let (offset, step) = steps[bit];
                    let next = distance + step;
                    let to = (from as i32 + offset) as usize;
                    if next <= REACH && next < costs[to] {
                        costs[to] = next;
                        wave[usize::from(next)].push(to);
                    }
                }
            }
        }
        costs
    }

    fn straight_cost(&self, tile: TileCoord) -> u16 {
        let dx = tile.x.abs_diff(self.center.x);
        let dy = tile.y.abs_diff(self.center.y);
        (dx.abs_diff(dy) * u32::from(ORTHOGONAL) + dx.min(dy) * u32::from(DIAGONAL)) as u16
    }

    fn index(&self, tile: TileCoord) -> usize {
        ((tile.y - self.terrain.bounds.min.y) * self.terrain.bounds.width() + tile.x
            - self.terrain.bounds.min.x) as usize
    }

    pub(super) fn cost(&self, tile: TileCoord) -> Option<u16> {
        if !self.terrain.bounds.contains(tile) {
            return None;
        }
        let cost = self
            .costs
            .as_ref()
            .map_or_else(|| self.straight_cost(tile), |costs| costs[self.index(tile)]);
        (cost <= REACH).then_some(cost)
    }

    pub(super) fn approach_cost(&self, tile: TileCoord) -> Option<u16> {
        self.cost(tile)
            .or_else(|| neighbors(tile).filter_map(|t| self.cost(t)).min())
    }

    pub(super) fn regions(&self, id: EntityId, walking: &super::Walking) -> Option<Vec<u32>> {
        let ground = self.ground.as_ref()?;
        let steps = tiles(self.terrain.bounds)
            .zip(ground)
            .map(|(tile, &mask)| mask.filter(|_| walking.owns(id, tile)))
            .collect::<Vec<_>>();
        Some(aoe_map::clipped_region_labels(
            self.terrain.bounds.width() as usize,
            &steps,
        ))
    }
}

pub(super) fn tiles(rect: TileRect) -> impl Iterator<Item = TileCoord> {
    (rect.min.y..rect.max.y)
        .flat_map(move |y| (rect.min.x..rect.max.x).map(move |x| TileCoord::new(x, y)))
}

fn center(origin: TileRect) -> TileCoord {
    TileCoord::new(
        (origin.min.x + origin.max.x) / 2,
        (origin.min.y + origin.max.y) / 2,
    )
}
