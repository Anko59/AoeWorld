//! Idle reaction to damage, including sources that are defensive buildings.
use super::{DamageKind, MELEE_RANGE_SUBUNITS, tick::Goal};
use crate::{GameWorld, UnitKind, UnitTask};
use aoe_core::{CombatStance, EntityId, TileCoord, WorldPosition};

impl GameWorld {
    pub(super) fn retaliate(
        &mut self,
        index: usize,
        attacker: EntityId,
        changed: &mut Vec<EntityId>,
    ) {
        let unit = self.units[index].state;
        if unit.hp == 0
            || unit.moving
            || unit.planning
            || !matches!(unit.task, UnitTask::Idle | UnitTask::Carrying)
            || self.combat.attackers.contains_key(&unit.id)
            || self.economy.gatherers.contains_key(&unit.id)
            || self.economy.builders.contains_key(&unit.id)
            || unit.stance == CombatStance::NoAttack
        {
            return;
        }
        let Some(goal) = self.attack_goal(attacker, unit.player) else {
            return;
        };
        let stats = self
            .unit_stats(unit.id)
            .unwrap_or(*super::unit_combat(unit.kind));
        let ranged = self.building(attacker).is_some()
            || self
                .unit_stats(attacker)
                .is_some_and(|s| s.attack_kind == DamageKind::Pierce || s.range > 1);
        let ram_target = self
            .unit(attacker)
            .is_some_and(|a| matches!(a.kind, UnitKind::BatteringRam | UnitKind::Mangonel))
            || self.building(attacker).is_some();
        let flee = stats.attack == 0
            || (unit.kind == UnitKind::Villager && ranged)
            || (unit.kind == UnitKind::BatteringRam && !ram_target);
        if unit.stance == CombatStance::StandGround {
            let range = super::super::navy::attack_range(&stats);
            if flee || goal.distance_squared(unit.position) > range * range {
                return;
            }
        }
        let los = self
            .unit_observer(&unit)
            .map_or(stats.los, |(_, _, los)| los);
        if unit.stance == CombatStance::Defensive
            && goal.distance_squared(unit.position) > (i64::from(los) * MELEE_RANGE_SUBUNITS).pow(2)
        {
            return;
        }
        if flee {
            self.flee_from(unit.id, goal);
        } else {
            self.begin_autonomous_attack(unit.id, attacker, unit.position);
        }
        changed.push(unit.id);
    }

    /// Eight local escape endpoints, with ordinary bounded movement planning.
    fn flee_from(&mut self, id: EntityId, danger: Goal) {
        let Some(unit) = self.unit(id) else {
            return;
        };
        let from = unit.position.tile_floor();
        let mut candidates = Vec::new();
        for (dx, dy) in [
            (0, -1),
            (-1, 0),
            (1, 0),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ] {
            let tile = TileCoord::new(from.x + dx * 3, from.y + dy * 3);
            let Ok(at) = WorldPosition::from_tile_center(tile) else {
                continue;
            };
            if !self.config.valid_map_position(at)
                || self.economy.blocked_tiles.contains(&tile)
                || !self.unit_step_clear(unit.kind, tile, tile).unwrap_or(false)
            {
                continue;
            }
            let d = danger.distance_squared(at);
            if d > danger.distance_squared(unit.position) {
                candidates.push((std::cmp::Reverse(d), tile.y, tile.x, at));
            }
        }
        candidates.sort_unstable_by_key(|&(d, y, x, _)| (d, y, x));
        for (_, _, _, at) in candidates {
            if self.issue_task_move(id, at).is_ok() {
                break;
            }
        }
    }
}
