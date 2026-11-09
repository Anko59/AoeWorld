use super::*;

impl GameWorld {
    pub(super) fn formation_members(
        &self,
        player: PlayerId,
        ids: &[EntityId],
    ) -> Result<(Vec<EntityId>, WorldPosition), EconomyError> {
        if ids.is_empty() || ids.len() > MAX_FORMATION_MEMBERS {
            return Err(EconomyError::OrderTooLarge);
        }
        let mut ids = ids.to_vec();
        ids.sort_unstable();
        if ids.windows(2).any(|p| p[0] == p[1]) {
            return Err(EconomyError::OrderTooLarge);
        }
        // A selection is sent as the player saw it; a member that fell before
        // the order arrived is left out rather than refusing the whole army.
        ids.retain(|&id| self.unit(id).is_some_and(|u| u.hp > 0));
        if ids.is_empty() {
            return Err(EconomyError::UnknownEntity);
        }
        let mut x = 0_i64;
        let mut y = 0_i64;
        for &id in &ids {
            let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
            if unit.player != player {
                return Err(EconomyError::NotOwner);
            }
            if self.embarked(id) {
                return Err(EconomyError::InvalidTarget);
            }
            if matches!(
                unit.kind,
                UnitKind::Villager | UnitKind::Vassal | UnitKind::Steward
            ) || unit.kind.is_naval()
            {
                return Err(EconomyError::WrongUnitKind);
            }
            x += i64::from(unit.position.x);
            y += i64::from(unit.position.y);
        }
        let centroid =
            WorldPosition::new((x / ids.len() as i64) as i32, (y / ids.len() as i64) as i32);
        Ok((ids, centroid))
    }

    pub(super) fn formation_origin(
        &self,
        ids: &[EntityId],
        centroid: WorldPosition,
    ) -> Result<WorldPosition, EconomyError> {
        let player = self.unit(ids[0]).ok_or(EconomyError::UnknownEntity)?.player;
        // The army starts in the land region most of its members stand in: a
        // spread selection's centroid can fall in a sealed pocket (the middle
        // of a village), and an order from there reached nothing.
        let walkable = self
            .unit_step_clear(
                UnitKind::Militia,
                centroid.tile_floor(),
                centroid.tile_floor(),
            )
            .map_err(GameWorldError::Environment)?
            && self.building_step_clear(centroid.tile_floor(), centroid.tile_floor());
        if walkable {
            let mut common = true;
            for &id in ids {
                let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
                if !self.land_reachable(
                    player,
                    unit.position.tile_floor(),
                    centroid.tile_floor(),
                )? || !self.land_reachable(
                    player,
                    centroid.tile_floor(),
                    unit.position.tile_floor(),
                )? {
                    common = false;
                    break;
                }
            }
            if common {
                return Ok(centroid);
            }
        }
        let region = self.formation_region(player, ids)?;
        if walkable
            && (region.is_none()
                || self.land_player_region(player, centroid.tile_floor())? == region)
        {
            return Ok(centroid);
        }
        let mut nearest = None;
        for &id in ids {
            let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
            if region.is_some()
                && self.land_player_region(player, unit.position.tile_floor())? != region
            {
                continue;
            }
            let dx = i64::from(unit.position.x) - i64::from(centroid.x);
            let dy = i64::from(unit.position.y) - i64::from(centroid.y);
            let key = (dx * dx + dy * dy, id, unit.position);
            if nearest
                .as_ref()
                .is_none_or(|best: &(i64, EntityId, WorldPosition)| {
                    (key.0, key.1) < (best.0, best.1)
                })
            {
                nearest = Some(key);
            }
        }
        nearest
            .map(|(_, _, point)| point)
            .or_else(|| {
                ids.first()
                    .and_then(|&id| self.unit(id))
                    .map(|u| u.position)
            })
            .ok_or(EconomyError::UnknownEntity)
    }

    /// The land region holding most members (ties: the lowest region), or
    /// `None` on ground without obstacles, where every point is reachable.
    fn formation_region(
        &self,
        player: PlayerId,
        ids: &[EntityId],
    ) -> Result<Option<u32>, EconomyError> {
        if !self.has_route_navigation() {
            return Ok(None);
        }
        let mut counts = BTreeMap::<u32, usize>::new();
        for unit in ids.iter().filter_map(|&id| self.unit(id)) {
            if let Some(region) = self.land_player_region(player, unit.position.tile_floor())? {
                *counts.entry(region).or_default() += 1;
            }
        }
        Ok(counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(region, _)| region))
    }

    /// Lay the army out around the nearest reachable frontier. The whole
    /// front must fit outside a wall: centring the block on its nearest tile
    /// would squeeze front rows against the wall and leave them pressing.
    pub(super) fn formation_goal(
        &self,
        player: PlayerId,
        origin: WorldPosition,
        destination: WorldPosition,
        form: Formation,
        ids: &[EntityId],
    ) -> Result<WorldPosition, EconomyError> {
        if !self.has_route_navigation()
            || self.land_reachable(player, origin.tile_floor(), destination.tile_floor())?
        {
            return Ok(destination);
        }
        let Some(tile) =
            self.land_frontier(player, origin.tile_floor(), destination.tile_floor())?
        else {
            return Ok(destination);
        };
        let frontier = WorldPosition::from_tile_center(tile).unwrap_or(destination);
        let heading = WorldPosition::new(frontier.x - origin.x, frontier.y - origin.y);
        let (gap, _) = self.formation_gaps(ids);
        let slots: Vec<_> = self
            .general_role_slots(ids.iter().copied(), gap)
            .map_or_else(
                || {
                    layout::slots(form, ids.len())
                        .into_iter()
                        .map(|p| WorldPosition::new(p.x * gap / 1040, p.y * gap / 1040))
                        .collect()
                },
                |roles| roles.into_iter().map(|(_, point)| point).collect(),
            );
        let largest = ids
            .iter()
            .map(|id| self.units[self.lookup[id]].state.kind)
            .max_by_key(|kind| kind.collision_radius())
            .unwrap_or(UnitKind::Militia);
        let extent = slots
            .iter()
            .map(|p| i64::from(p.x.abs()) + i64::from(p.y.abs()))
            .max()
            .unwrap_or(0);
        let distance = path::distance(origin, frontier);
        // Search the approach band occupied by this layout, in half-tile
        // increments. If no complete layout fits, ordinary tight packing
        // still places the block at the frontier.
        for retreat in 0..=(extent + i64::from(gap)) / 512 {
            let anchor = path::step(
                frontier,
                origin,
                (retreat * 512).min(distance).min(i64::from(i32::MAX)) as i32,
            );
            let mut fits = true;
            for &offset in &slots {
                let point = layout::oriented(anchor, offset, heading);
                if !self.config.valid_map_position(point)
                    || !self.body_position_clear(largest, point)
                    || !self.land_reachable(player, origin.tile_floor(), point.tile_floor())?
                {
                    fits = false;
                    break;
                }
            }
            if fits {
                return Ok(anchor);
            }
        }
        Ok(frontier)
    }

    pub(super) fn formation_gaps(&self, ids: &[EntityId]) -> (i32, i32) {
        // Leave room for a late member to pass occupied slots, plus one step.
        let (diameter, step) = ids.iter().fold((0, 0), |(diameter, step), id| {
            let unit = self.units[self.lookup[id]].state;
            let (speed, denominator) = self.movement_speed(unit.kind, unit.player);
            (
                diameter.max(unit.kind.collision_radius() * 2),
                step.max(speed.div_ceil(denominator).min(512) as i32),
            )
        });
        (diameter * 2 + step + 16, diameter + step + 16)
    }

    /// Apply a posture now, retaining an existing group's route and FIFO.
    pub fn issue_formation(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        form: Formation,
    ) -> Result<(), EconomyError> {
        let (ids, centroid) = self.formation_members(player, ids)?;
        let carrier = self.units[self.lookup[&ids[0]]].formation.unwrap_or(ids[0]);
        if let Some(mut group) = self.formations.remove(&carrier) {
            if group.members.iter().map(|m| m.id).eq(ids.iter().copied()) {
                group.form = form;
                self.reset_group_detours(&mut group);
                self.assign_group_slots(&mut group);
                group.settled = false;
                self.formations.insert(carrier, group);
                for &id in &ids {
                    self.vassal_player_override(id);
                }
                return Ok(());
            }
            self.formations.insert(carrier, group);
        }
        let at = self.formation_origin(&ids, centroid)?;
        self.issue_formation_move(player, &ids, form, at, false, false)
    }
}

/// Diagnostics for the real-match probe: where a formation order of the
/// given units starts, its members' nominal speeds and the farthest slot.
#[derive(Clone, Debug)]
pub struct GroupProbe {
    pub origin: WorldPosition,
    /// Kinds of the members with their speed (subunits per tick, as a ratio).
    pub speeds: Vec<(UnitKind, (u64, u64))>,
    /// The slowest member's kind and speed: the group's nominal pace.
    pub slowest: (UnitKind, (u64, u64)),
    /// Farthest slot from the formation centre at its destination (subunits).
    pub extent: i64,
}

impl GameWorld {
    pub(crate) fn group_size(&self, leader: EntityId) -> Option<usize> {
        self.formations
            .get(&leader)
            .map(|group| group.members.len())
    }

    /// Read-only: what `issue_formation_move` would start from. Never
    /// changes state or counters.
    pub fn probe_group(
        &self,
        player: PlayerId,
        ids: &[EntityId],
        form: Formation,
    ) -> Result<GroupProbe, EconomyError> {
        let (ids, centroid) = self.formation_members(player, ids)?;
        let origin = self.formation_origin(&ids, centroid)?;
        let mut speeds = Vec::<(UnitKind, (u64, u64))>::new();
        for id in &ids {
            let kind = self.units[self.lookup[id]].state.kind;
            if !speeds.iter().any(|(k, _)| *k == kind) {
                speeds.push((kind, self.movement_speed(kind, player)));
            }
        }
        let slowest = *speeds
            .iter()
            .min_by(|a, b| {
                (u128::from(a.1.0) * u128::from(b.1.1))
                    .cmp(&(u128::from(b.1.0) * u128::from(a.1.1)))
            })
            .ok_or(EconomyError::UnknownEntity)?;
        let extent = layout::slots(form, ids.len())
            .iter()
            .map(|p| (i64::from(p.x).pow(2) + i64::from(p.y).pow(2)).isqrt())
            .max()
            .unwrap_or(0);
        Ok(GroupProbe {
            origin,
            speeds,
            slowest,
            extent,
        })
    }
}
