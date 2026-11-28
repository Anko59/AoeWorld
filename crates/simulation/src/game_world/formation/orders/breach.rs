//! A blocked attack-move marches to the reachable side of the obstruction,
//! fights there, then reuses the original waypoint once the breach is open.
use super::*;

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
    /// Whether the unit belongs to an army whose group, not the soldier, deals
    /// with the walls on its route: a lone unit has no group to do it.
    pub(crate) fn in_breaching_group(&self, id: EntityId) -> bool {
        self.units[self.lookup[&id]]
            .formation
            .and_then(|leader| self.formations.get(&leader))
            .is_some_and(|group| group.breach.is_some() || group.members.len() > 1)
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
            self.economy
                .buildings
                .get(id)
                .is_some_and(|building| building.player != player && building.hp > 0)
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

    pub(super) fn formation_leg(
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
            if self.land_reachable(player, bucket, from, target)? {
                return Ok((requested, None));
            }
            let found = self.land_frontier(player, bucket, from, target)?;
            known = Some(found);
            if let Some(frontier) = found
                && let Some(blocker) = self.frontier_blocker(player, frontier, target)
                && ids.iter().any(|id| {
                    self.unit(*id)
                        .is_some_and(|u| crate::unit_combat(u.kind).attack > 0)
                })
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
