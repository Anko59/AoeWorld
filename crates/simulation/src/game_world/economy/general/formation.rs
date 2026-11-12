use super::*;
impl GameWorld {
    /// Reinforcements join the march without replacing admitted routes and
    /// final slots of soldiers already following the same ordinary goal.
    pub(super) fn general_march_retained(&self, id: EntityId, goal: WorldPosition) -> bool {
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        let unit = &self.units[index];
        // A scalar order, including the commander's own move, still needs
        // admission to its army formation and role slot.
        unit.state.attack_target.is_none()
            && self.movement_failure(id).is_none()
            && self.formation_keeps_march(id, goal)
    }

    /// A squad cut off from both its commander and its goal must retry after
    /// an opening, rather than completing a local frontier formation.
    pub(super) fn general_squad_goal_reachable(
        &self,
        ids: &[EntityId],
        goal: WorldPosition,
        commander: WorldPosition,
    ) -> Result<bool, crate::GameWorldError> {
        let bucket = ids
            .iter()
            .filter_map(|&id| self.unit(id))
            .filter_map(|unit| aoe_map::LandBucket::for_radius(unit.kind.collision_radius()))
            .max()
            .unwrap_or(aoe_map::LandBucket::Small);
        for unit in ids.iter().filter_map(|&id| self.unit(id)) {
            if self.land_reachable(
                unit.player,
                bucket,
                unit.position.tile_floor(),
                goal.tile_floor(),
            )? || self.land_reachable(
                unit.player,
                bucket,
                unit.position.tile_floor(),
                commander.tile_floor(),
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn general_formation(
        &mut self,
        g: EntityId,
        army: &[GameUnit],
        to: WorldPosition,
        retreat: bool,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        self.march_squads(g, army, to, retreat, changed, work);
    }

    /// Role offsets enter the block's ordinary destination assignment, before
    /// clearance validation, ranks, forming distances and route cursors are set.
    pub(crate) fn general_role_slots(
        &self,
        ids: impl Iterator<Item = EntityId> + Clone,
        gap: i32,
    ) -> Option<Vec<(EntityId, WorldPosition)>> {
        if self.economy.generals.armies.is_empty() {
            return None;
        }
        let general = ids
            .clone()
            .find(|id| self.economy.generals.armies.contains_key(id))?;
        if !ids
            .clone()
            .all(|id| id == general || self.general_controller(id) == Some(general))
        {
            return None;
        }
        let mut ranked: Vec<_> = ids.filter_map(|id| self.unit(id)).collect();
        ranked.sort_unstable_by_key(|u| u.id);
        let mut counts = [0; 6];
        let width = ranked.len().isqrt().max(4) as i32;
        let infantry = ranked.iter().filter(|u| role(u.kind) == Some(0)).count();
        let depth = infantry.div_ceil(width as usize).max(3) as i32;
        let archer_rows = ranked
            .iter()
            .filter(|u| role(u.kind) == Some(1))
            .count()
            .div_ceil(width as usize) as i32;
        let mut slots = Vec::new();
        for u in ranked {
            let r = role(u.kind).unwrap_or(5);
            let n = counts[r];
            counts[r] += 1;
            let row = n / width;
            let column = n % width - width / 2;
            let (x, y) = match r {
                0 => (column, depth - row),
                1 => (column, -1 - row),
                2 => (
                    if n % 2 == 0 {
                        -(width / 2 + 2 + (n / 2) % 3)
                    } else {
                        width / 2 + 2 + (n / 2) % 3
                    },
                    depth - n / 6,
                ),
                3 => (column, -archer_rows - 2 - row),
                4 => (column, depth + 4 + row),
                _ => (0, 0),
            };
            slots.push((u.id, WorldPosition::new(x * gap, y * gap)));
        }
        Some(slots)
    }
}
