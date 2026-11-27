//! A construction site is usable from a walkable perimeter tile.

use super::*;
use crate::GameWorldError;
use aoe_map::LandBucket;

const NEARBY_VILLAGER_TILES: i32 = 16;
/// Perimeter tiles one approach tries before reporting failure.
const APPROACH_TILE_TRIES: usize = 6;

impl GameWorld {
    pub(crate) fn forget_foundation_worker(&mut self, unit: crate::GameUnit) {
        if unit.kind == UnitKind::Villager
            && let Some(workers) = self.economy.villagers.get_mut(&unit.player)
        {
            workers.remove(&unit.id);
            if workers.is_empty() {
                self.economy.villagers.remove(&unit.player);
            }
        }
    }

    /// Check before spending resources or creating a reservation. Most sites
    /// are served by the named or nearby workers. A missed local search reads
    /// only the owner's villager index, including detached settlements.
    pub(in super::super) fn validate_foundation_reach(
        &self,
        player: PlayerId,
        preferred: &[EntityId],
        footprint: TileRect,
    ) -> Result<(), EconomyError> {
        let Some(workers) = self.economy.villagers.get(&player) else {
            return Err(GameWorldError::Unreachable.into());
        };
        let mut goals = perimeter(footprint);
        goals.retain(|&tile| self.tile_passable(tile));
        if goals.is_empty() {
            return Err(GameWorldError::Unreachable.into());
        }
        let bucket = villager_bucket()?;
        // Villagers can share a tile. Prove each distinct route origin once.
        let mut tested = std::collections::BTreeSet::new();
        let mut check = |unit: crate::GameUnit| -> Result<bool, GameWorldError> {
            if unit.hp == 0
                || unit.kind != UnitKind::Villager
                || unit.player != player
                || self.embarked(unit.id)
            {
                return Ok(false);
            }
            let from = self.route_origin(unit.kind, unit.position).tile_floor();
            if !tested.insert(from) {
                return Ok(false);
            }
            for &goal in &goals {
                if self.land_reachable(player, bucket, from, goal)? {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        for unit in preferred.iter().filter_map(|&id| self.unit(id)) {
            if check(unit)? {
                return Ok(());
            }
        }
        let r = NEARBY_VILLAGER_TILES;
        let near = TileRect::from_xywh(
            footprint.min.x - r,
            footprint.min.y - r,
            footprint.width() + 2 * r,
            footprint.height() + 2 * r,
        )
        .clamp(self.config.width_tiles, self.config.height_tiles);
        for unit in self.query(near).0 {
            if check(unit)? {
                return Ok(());
            }
        }
        for unit in workers.iter().filter_map(|&id| self.unit(id)) {
            if !near.contains(unit.position.tile_floor()) && check(unit)? {
                return Ok(());
            }
        }
        Err(GameWorldError::Unreachable.into())
    }

    /// Walk to a reachable contact beside `footprint`. Rotate the choices
    /// when a previous attempt could not use its point.
    pub(super) fn approach_building(
        &mut self,
        id: EntityId,
        footprint: TileRect,
        skip: usize,
    ) -> Result<(), GameWorldError> {
        let unit = self.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        let origin = self.route_origin(unit.kind, unit.position).tile_floor();
        let ship = unit.kind == UnitKind::FishingShip;
        let mut candidates = perimeter(footprint);
        candidates.retain(|tile| {
            if ship {
                self.unit_tile_passable(UnitKind::FishingShip, *tile, &|| false)
                    .unwrap_or(false)
            } else {
                self.tile_passable(*tile)
            }
        });
        candidates.sort_by_key(|tile| {
            (
                self.work_spot_busy(*tile, id),
                distance_squared(origin, *tile),
                tile.y,
                tile.x,
            )
        });
        if !candidates.is_empty() {
            let by = skip % candidates.len();
            candidates.rotate_left(by);
        }
        let mut failure = GameWorldError::Unreachable;
        // Unreachable contact points use no movement attempts.
        let bucket = villager_bucket()?;
        let mut tried = 0;
        for tile in candidates {
            if !ship && !self.land_reachable(unit.player, bucket, origin, tile)? {
                continue;
            }
            tried += 1;
            match self.issue_task_move(id, position(tile)?) {
                Ok(_) => return Ok(()),
                Err(error @ GameWorldError::Environment(_)) => return Err(error),
                Err(error) => failure = error,
            }
            if tried == APPROACH_TILE_TRIES {
                break;
            }
        }
        Err(failure)
    }
}

fn villager_bucket() -> Result<LandBucket, GameWorldError> {
    LandBucket::for_radius(UnitKind::Villager.collision_radius()).ok_or(GameWorldError::Unreachable)
}
