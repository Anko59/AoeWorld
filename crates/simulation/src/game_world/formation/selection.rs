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
        // A centroid inside a tree/building is projected to the nearest member;
        // the ordinary scheduler must start on walkable ground.
        Ok(
            if self
                .unit_step_clear(
                    UnitKind::Militia,
                    centroid.tile_floor(),
                    centroid.tile_floor(),
                )
                .unwrap_or(false)
                && self.building_step_clear(centroid.tile_floor(), centroid.tile_floor())
            {
                centroid
            } else {
                ids.iter()
                    .filter_map(|&id| self.unit(id))
                    .min_by_key(|u| {
                        let dx = i64::from(u.position.x) - i64::from(centroid.x);
                        let dy = i64::from(u.position.y) - i64::from(centroid.y);
                        dx * dx + dy * dy
                    })
                    .ok_or(EconomyError::UnknownEntity)?
                    .position
            },
        )
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
