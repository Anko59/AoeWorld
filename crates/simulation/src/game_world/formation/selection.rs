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
        let mut x = 0_i64;
        let mut y = 0_i64;
        for &id in &ids {
            let unit = self
                .unit(id)
                .filter(|u| u.hp > 0)
                .ok_or(EconomyError::UnknownEntity)?;
            if unit.player != player {
                return Err(EconomyError::NotOwner);
            }
            if self.embarked(id) {
                return Err(EconomyError::InvalidTarget);
            }
            if unit.kind == UnitKind::Villager || unit.kind.is_naval() {
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
        if let Some(group) = self.formations.get_mut(&ids[0])
            && group.members.iter().map(|m| m.id).eq(ids.iter().copied())
        {
            group.form = form;
            for (member, offset) in group.members.iter_mut().zip(layout::slots(form, ids.len())) {
                member.offset = offset;
                member.cache = None;
                member.probe = 0;
            }
            group.ready = false;
            group.settled = false;
            return Ok(());
        }
        let at = self.formation_origin(&ids, centroid)?;
        self.issue_formation_move(player, &ids, form, at, false, false)
    }
}
