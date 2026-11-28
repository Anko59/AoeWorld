//! A blocked attack-move marches to the reachable side of the obstruction,
//! fights there, then reuses the original waypoint once the breach is open.
use super::*;
use crate::game_world::unit_index::presence::PRESENCE_CELL_TILES;

impl Group {
    pub(super) fn resume_at(&self, slot: WorldPosition) -> Resume {
        let destination = if self.breach.is_some() {
            self.marker.position
        } else {
            slot
        };
        Resume::new(destination, self.marker)
    }
}

impl GameWorld {
    /// Marching formations, including one-member formations, select the
    /// obstructions on their own route.
    pub(crate) fn in_breaching_group(&self, id: EntityId) -> bool {
        self.units[self.lookup[&id]]
            .formation
            .and_then(|leader| self.formations.get(&leader))
            .is_some_and(|group| (group.attack_move && !group.settled) || group.members.len() > 1)
    }

    pub(crate) fn hostile_ground_blocker(
        &self,
        player: PlayerId,
        tile: TileCoord,
    ) -> Option<EntityId> {
        if self.route_tile_admitted(player, tile) {
            return None;
        }
        self.building_at_tile(tile).filter(|id| {
            self.economy.buildings.get(id).is_some_and(|building| {
                self.is_enemy(player, building.player)
                    && building.hp > 0
                    && !building.is_unstarted()
            })
        })
    }

    pub(super) fn breach_still_blocks(&self, player: PlayerId, id: EntityId) -> bool {
        self.economy.buildings.get(&id).is_some_and(|building| {
            self.hostile_ground_blocker(player, building.footprint.min) == Some(id)
        })
    }

    pub(super) fn validate_formation_destination(
        &self,
        player: PlayerId,
        destination: WorldPosition,
        attack_move: bool,
    ) -> Result<(), EconomyError> {
        let tile = destination.tile_floor();
        // On mapped terrain the building is also part of the terrain
        // collision overlay, so only check it as ground after this exception.
        if attack_move && self.hostile_ground_blocker(player, tile).is_some() {
            return Ok(());
        }
        if !self
            .unit_tile_passable(UnitKind::Militia, tile, &|| false)
            .map_err(GameWorldError::Environment)?
            || !self.route_tile_admitted(player, tile)
        {
            return Err(GameWorldError::InvalidPosition.into());
        }
        Ok(())
    }

    pub(in crate::game_world) fn formation_leg(
        &self,
        player: PlayerId,
        origin: WorldPosition,
        requested: WorldPosition,
        form: Formation,
        ids: &[EntityId],
        attack_move: bool,
    ) -> Result<(WorldPosition, Option<EntityId>), EconomyError> {
        let mut known = None;
        if attack_move && self.has_route_navigation() {
            let bucket = self.formation_bucket(ids);
            let from = origin.tile_floor();
            let target = requested.tile_floor();
            let can_breach = || {
                ids.iter().any(|&id| {
                    let unit = self.units[self.lookup[&id]].state;
                    unit.stance != aoe_core::CombatStance::NoAttack
                        && crate::unit_combat(unit.kind).attack > 0
                })
            };
            if let Some((approach, blocker)) = self.direct_blocker(player, origin, requested)
                && can_breach()
                && self.land_reachable(player, bucket, from, approach)?
            {
                let at = WorldPosition::from_tile_center(approach)
                    .map_err(|_| GameWorldError::InvalidPosition)?;
                return Ok((at, Some(blocker)));
            }
            if self.land_reachable(player, bucket, from, target)? {
                return Ok((requested, None));
            }
            let found = self.land_frontier(player, bucket, from, target)?;
            known = Some(found);
            if let Some(frontier) = found
                && let Some(blocker) = self.frontier_blocker(player, frontier, target)
                && can_breach()
            {
                return Ok((
                    WorldPosition::from_tile_center(frontier)
                        .map_err(|_| GameWorldError::InvalidPosition)?,
                    Some(blocker),
                ));
            }
        }
        if !self.route_tile_admitted(player, requested.tile_floor()) {
            return Err(GameWorldError::InvalidPosition.into());
        }
        self.formation_goal_from(player, origin, requested, form, ids, known)
            .map(|goal| (goal, None))
    }

    /// Find the first hostile footprint on the original march, before the
    /// route planner can send the group to another side of that footprint.
    /// Compare boundary-crossing times as integers. At a corner both side
    /// tiles can block the diagonal, even though the line only touches them.
    pub(in crate::game_world) fn direct_blocker(
        &self,
        player: PlayerId,
        from: WorldPosition,
        to: WorldPosition,
    ) -> Option<(TileCoord, EntityId)> {
        let mut at = from.tile_floor();
        let end = to.tile_floor();
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        let sx = dx.signum() as i32;
        let sy = dy.signum() as i32;
        let tile = i64::from(FIXED_SUBUNITS_PER_TILE);
        let boundary = |cell: i32, step: i32, source: i32| {
            (i64::from(cell + i32::from(step > 0)) * tile - i64::from(source)).abs()
        };
        let mut x = boundary(at.x, sx, from.x) * dy.abs();
        let mut y = boundary(at.y, sy, from.y) * dx.abs();
        let x_stride = tile * dy.abs();
        let y_stride = tile * dx.abs();
        let mut presence = None;
        let mut blocker_at = |tile: TileCoord| {
            let cell = ChunkCoord {
                x: tile.x.div_euclid(PRESENCE_CELL_TILES),
                y: tile.y.div_euclid(PRESENCE_CELL_TILES),
            };
            if presence.is_none_or(|(last, _)| last != cell) {
                presence = Some((
                    cell,
                    self.enemies_present(&self.vision.building_cells, cell, player),
                ));
            }
            if presence.is_some_and(|(_, hostile)| hostile) {
                self.hostile_ground_blocker(player, tile)
            } else {
                None
            }
        };
        while at != end {
            let tx = if at.x == end.x { i64::MAX } else { x };
            let ty = if at.y == end.y { i64::MAX } else { y };
            let previous = at;
            if tx == ty {
                let a = blocker_at(TileCoord::new(at.x + sx, at.y));
                let b = blocker_at(TileCoord::new(at.x, at.y + sy));
                if let Some(id) = a.into_iter().chain(b).min() {
                    return Some((previous, id));
                }
            }
            if tx <= ty {
                at.x += sx;
                x += x_stride;
            }
            if ty <= tx {
                at.y += sy;
                y += y_stride;
            }
            if let Some(id) = blocker_at(at) {
                return Some((previous, id));
            }
        }
        None
    }

    /// Land-region clearance can leave a large body more than a tile from a
    /// footprint. Look only around its nearest reachable point, toward the
    /// destination, so an unrelated nearby building does not stall a march.
    fn frontier_blocker(
        &self,
        player: PlayerId,
        frontier: TileCoord,
        target: TileCoord,
    ) -> Option<EntityId> {
        let distance = |a: TileCoord, b: TileCoord| {
            (i64::from(a.x) - i64::from(b.x)).pow(2) + (i64::from(a.y) - i64::from(b.y)).pow(2)
        };
        let limit = distance(frontier, target);
        let clicked = self.building_at_tile(target);
        let mut best = None;
        for y in frontier.y - 2..=frontier.y + 2 {
            for x in frontier.x - 2..=frontier.x + 2 {
                let at = TileCoord::new(x, y);
                let toward = distance(at, target);
                if toward < limit
                    && let Some(id) = self.hostile_ground_blocker(player, at)
                {
                    let key = (Some(id) != clicked, distance(at, frontier), toward, id);
                    if best.is_none_or(|other| key < other) {
                        best = Some(key);
                    }
                }
            }
        }
        best.map(|(_, _, _, id)| id)
    }
}
