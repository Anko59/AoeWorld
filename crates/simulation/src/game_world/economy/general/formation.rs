use super::*;
impl GameWorld {
    pub(super) fn general_formation(
        &mut self,
        g: EntityId,
        army: &[GameUnit],
        to: WorldPosition,
        retreat: bool,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        if self.economy.generals.armies[&g].routed == Some(to) {
            return;
        }
        let ids: Vec<_> = army.iter().map(|v| v.id).collect();
        let player = army[0].player;
        work.orders += 1;
        if self
            .issue_formation_move(player, &ids, aoe_core::Formation::Line, to, false, false)
            .is_err()
        {
            return;
        }
        let _ = self.issue_stance(
            player,
            &ids,
            if retreat {
                aoe_core::CombatStance::NoAttack
            } else {
                aoe_core::CombatStance::Defensive
            },
        );
        if let Some(a) = self.economy.generals.armies.get_mut(&g) {
            a.routed = Some(to);
        }
        changed.extend(ids);
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
