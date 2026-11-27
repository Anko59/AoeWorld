//! Walking certificates for a Monk's approach to a known Monastery.
use super::World;
use aoe_core::{EntityId, TileCoord, TileRect};
use aoe_map::{LandBucket, RegionGround, RegionTile, Regions};
use aoe_protocol::{BuildingKind, ConstructionState};
use std::collections::HashSet;

/// Use the same bounded label search as simulation movement.
const PROOF_NODES: u32 = 64;

impl World {
    /// First candidate with a proven walk from `from` to its outside ring.
    /// Candidates are supplied in preference order. The occupied centre of
    /// a Monastery is never a destination. Unknown terrain cannot prove a walk.
    pub fn first_reachable_monastery(
        &self,
        from: TileCoord,
        candidates: &[EntityId],
    ) -> Option<EntityId> {
        let player = self.me?;
        if candidates.is_empty() || !self.land_known(from) {
            return None;
        }
        let ground = ObservedWalkingGround::new(self);
        let bucket = LandBucket::for_radius(aoe_simulation::UnitKind::Monk.collision_radius())?;
        let mut regions = Regions::new(self.width, self.height, bucket);
        candidates.iter().copied().find(|id| {
            let Some(b) = self.buildings.get(id).filter(|b| {
                b.player == player
                    && b.kind == BuildingKind::Monastery
                    && b.construction == ConstructionState::Complete
            }) else {
                return false;
            };
            let mut goals = approach_tiles(b.footprint).collect::<Vec<_>>();
            goals.sort_unstable_by_key(|tile| {
                let dx = i64::from(tile.x) - i64::from(from.x);
                let dy = i64::from(tile.y) - i64::from(from.y);
                (dx * dx + dy * dy, *tile)
            });
            goals.into_iter().any(|goal| {
                ground.tile(goal) == RegionTile::Open
                    && regions
                        .reachable_bounded(&ground, u32::from(player.0), from, goal, PROOF_NODES)
                        .is_reachable()
            })
        })
    }
}

fn approach_tiles(rect: TileRect) -> impl Iterator<Item = TileCoord> {
    (rect.min.y - 1..=rect.max.y).flat_map(move |y| {
        (rect.min.x - 1..=rect.max.x)
            .map(move |x| TileCoord::new(x, y))
            .filter(move |&tile| !rect.contains(tile))
    })
}

struct ObservedWalkingGround<'a> {
    world: &'a World,
    blocked: HashSet<TileCoord>,
}

impl<'a> ObservedWalkingGround<'a> {
    fn new(world: &'a World) -> Self {
        let mut blocked: HashSet<_> = world
            .nodes()
            .filter(|node| node.blocks_walk && world.land_known(node.tile))
            .filter(|node| world.amounts.get(&node.id).copied().unwrap_or(node.initial) > 0)
            .map(|node| node.tile)
            .collect();
        for b in world
            .buildings
            .values()
            .filter(|b| b.hp > 0 && !b.gate_open)
        {
            let Some(rect) = crate::rules::native_building(b.kind)
                .collision_rect(b.footprint, b.construction == ConstructionState::Complete)
            else {
                continue;
            };
            blocked.extend((rect.min.y..rect.max.y).flat_map(|y| {
                (rect.min.x..rect.max.x)
                    .map(move |x| TileCoord::new(x, y))
                    .filter(|&tile| b.contains_tile(tile))
            }));
        }
        Self { world, blocked }
    }
}

impl RegionGround for ObservedWalkingGround<'_> {
    fn tile(&self, tile: TileCoord) -> RegionTile {
        if self.world.land_known(tile) && !self.blocked.contains(&tile) {
            RegionTile::Open
        } else {
            RegionTile::Blocked
        }
    }

    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        self.world.bridges.bridge_edge(from, to).unwrap_or_else(|| {
            self.world.map_hash.is_none() || self.world.catalog.land_edge(from, to)
        })
    }

    fn diagonal_override(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        self.world.bridges.bridge_edge(from, to)
    }

    fn gate_owner(&self, _: u32) -> u32 {
        aoe_map::ANY_PLAYER
    }
}
