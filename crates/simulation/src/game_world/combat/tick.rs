//! Per-tick attack resolution for units that hold an attack order.
use super::{Attacker, HitRecord, MELEE_RANGE_SUBUNITS, damage_dealt, unit_combat};
use crate::game_world::{
    economy::placement::{perimeter, position},
    facing_for,
};
use crate::{GameUnit, GameWorld, UnitTask};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, Tick, TileRect, WorldPosition};

/// What an attacker is walking to and hitting.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Goal {
    Unit(WorldPosition),
    Building(TileRect),
}

impl Goal {
    pub(crate) fn distance_squared(&self, from: WorldPosition) -> i64 {
        let (x, y) = (i64::from(from.x), i64::from(from.y));
        let (cx, cy) = match *self {
            Goal::Unit(at) => (i64::from(at.x), i64::from(at.y)),
            Goal::Building(rect) => {
                let tile = i64::from(FIXED_SUBUNITS_PER_TILE);
                (
                    x.clamp(i64::from(rect.min.x) * tile, i64::from(rect.max.x) * tile),
                    y.clamp(i64::from(rect.min.y) * tile, i64::from(rect.max.y) * tile),
                )
            }
        };
        (x - cx) * (x - cx) + (y - cy) * (y - cy)
    }
    fn centre(&self) -> WorldPosition {
        match *self {
            Goal::Unit(at) => at,
            Goal::Building(rect) => WorldPosition::new(
                (rect.min.x + rect.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
                (rect.min.y + rect.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
            ),
        }
    }
}

impl GameWorld {
    /// A live unit or building of another player, else `None`.
    pub(crate) fn attack_goal(&self, target: EntityId, attacker: PlayerId) -> Option<Goal> {
        if let Some(unit) = self.unit(target) {
            return (unit.hp > 0 && unit.player != attacker).then_some(Goal::Unit(unit.position));
        }
        if let Some(animal) = self.animal(target) {
            return (animal.hp > 0).then_some(Goal::Unit(animal.position));
        }
        self.economy
            .buildings
            .get(&target)
            .filter(|building| building.hp > 0 && building.player != attacker)
            .map(|building| Goal::Building(building.footprint))
    }

    /// Unit targets are approached directly; buildings through the nearest
    /// walkable tile of their perimeter.
    pub(crate) fn attack_destination(
        &self,
        from: WorldPosition,
        goal: &Goal,
    ) -> Option<WorldPosition> {
        match *goal {
            Goal::Unit(at) => Some(at),
            Goal::Building(rect) => perimeter(rect)
                .into_iter()
                .filter(|tile| self.tile_passable(*tile))
                .filter_map(|tile| {
                    let at = position(tile).ok()?;
                    let (dx, dy) = (i64::from(at.x - from.x), i64::from(at.y - from.y));
                    Some((dx * dx + dy * dy, tile, at))
                })
                .min_by_key(|(distance, tile, _)| (*distance, tile.y, tile.x))
                .map(|(_, _, at)| at),
        }
    }

    pub(crate) fn advance_combat(&mut self, changed: &mut Vec<GameUnit>) {
        self.combat.stats = Default::default();
        let now = self.tick.0;
        while let Some(&(due, id)) = self.combat.dying.first() {
            if due > now {
                break;
            }
            self.combat.dying.pop_first();
            self.remove_unit(id);
        }
        // Ids are snapshotted: a unit that starts retaliating this tick first
        // acts on the next one, which keeps the order canonical.
        let ids = self.combat.attackers.keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.step_attacker(id, now, changed);
        }
        self.advance_towers(changed);
    }

    fn step_attacker(&mut self, id: EntityId, now: u64, changed: &mut Vec<GameUnit>) {
        let (Some(&index), Some(attacker)) = (
            self.lookup.get(&id),
            self.combat.attackers.get(&id).copied(),
        ) else {
            self.combat.attackers.remove(&id);
            return;
        };
        let before = self.units[index].state;
        self.combat.stats.attackers += 1;
        let Some(goal) = self.attack_goal(attacker.target, before.player) else {
            self.end_attack(id, changed);
            return;
        };
        if goal.distance_squared(before.position) > MELEE_RANGE_SUBUNITS * MELEE_RANGE_SUBUNITS {
            self.approach(id, index, attacker, goal, now, changed);
            return;
        }
        if self.units[index].order.is_some() || before.moving || before.planning {
            self.stop_movement(index);
        }
        let (centre, position) = (goal.centre(), before.position);
        let state = &mut self.units[index].state;
        state.facing = facing_for(
            i64::from(centre.x) - i64::from(position.x),
            i64::from(centre.y) - i64::from(position.y),
            state.facing,
        );
        state.task = UnitTask::Attacking;
        if now >= self.units[index].next_hit_tick {
            self.units[index].next_hit_tick = now + u64::from(self.attack_period_ticks(&before));
            self.strike(id, attacker.target, changed);
        }
        if let Some(&index) = self.lookup.get(&id)
            && self.units[index].state != before
        {
            changed.push(self.units[index].state);
        }
    }

    fn approach(
        &mut self,
        id: EntityId,
        index: usize,
        attacker: Attacker,
        goal: Goal,
        now: u64,
        changed: &mut Vec<GameUnit>,
    ) {
        let before = self.units[index].state;
        let order = self.units[index].order;
        if order.is_none() && self.units[index].last_movement_error.is_some() {
            self.end_attack(id, changed);
            return;
        }
        // The perimeter search runs only when the repath timer has expired.
        if now >= attacker.repath_tick {
            let destination = self.attack_destination(before.position, &goal);
            let stale = match (order, destination) {
                (None, _) => true,
                (Some(order), Some(at)) => order.target_tile != at.tile_floor(),
                (Some(_), None) => false,
            };
            if stale {
                self.repath_attacker(id, destination, now, changed);
                if !self.combat.attackers.contains_key(&id) {
                    return;
                }
            }
        }
        self.units[index].state.task = UnitTask::Attacking;
        if self.units[index].state != before {
            changed.push(self.units[index].state);
        }
    }

    fn repath_attacker(
        &mut self,
        id: EntityId,
        destination: Option<WorldPosition>,
        now: u64,
        changed: &mut Vec<GameUnit>,
    ) {
        let Some(at) = destination else {
            self.end_attack(id, changed);
            return;
        };
        let repath = now.saturating_add(self.repath_ticks());
        if let Some(entry) = self.combat.attackers.get_mut(&id) {
            entry.repath_tick = repath;
        }
        if matches!(
            self.issue_task_move(id, at),
            Err(crate::GameWorldError::Unreachable | crate::GameWorldError::InvalidPosition)
        ) {
            self.end_attack(id, changed);
        }
    }

    /// Stops the attack: the target is gone, unreachable or friendly.
    pub(crate) fn end_attack(&mut self, id: EntityId, changed: &mut Vec<GameUnit>) {
        self.combat.attackers.remove(&id);
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        if self.units[index].state.hp > 0 {
            self.stop_movement(index);
            let state = &mut self.units[index].state;
            state.attack_target = None;
            state.task = if state.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
        }
        changed.push(self.units[index].state);
    }

    fn strike(&mut self, attacker_id: EntityId, target: EntityId, changed: &mut Vec<GameUnit>) {
        let Some(&attacker_index) = self.lookup.get(&attacker_id) else {
            return;
        };
        let stats = *unit_combat(self.units[attacker_index].state.kind);
        self.resolve_hit(
            attacker_id,
            target,
            stats.attack,
            stats.attack_kind,
            changed,
        );
    }

    /// Both melee and tower arrows share armour, death and retaliation logic.
    pub(super) fn resolve_hit(
        &mut self,
        attacker_id: EntityId,
        target: EntityId,
        attack: u16,
        kind: super::DamageKind,
        changed: &mut Vec<GameUnit>,
    ) {
        let hit = HitRecord {
            attacker: attacker_id,
            tick: Tick(self.tick.0),
        };
        self.combat.stats.hits += 1;
        if let Some(&victim_index) = self.lookup.get(&target) {
            let (melee, pierce) = self.unit_armour(target).unwrap_or_default();
            let victim = &mut self.units[victim_index].state;
            let damage = damage_dealt(attack, kind, melee, pierce);
            victim.hp = victim.hp.saturating_sub(damage);
            victim.last_hit = Some(hit);
            let (hp, victim_id) = (victim.hp, victim.id);
            if hp == 0 {
                self.kill_unit(victim_index);
                self.end_attack(attacker_id, changed);
            } else if self.can_retaliate(victim_index) {
                self.begin_attack(victim_id, attacker_id);
            }
            changed.push(self.units[victim_index].state);
        } else if self.animal(target).is_some() {
            if self.strike_animal(target, attacker_id, attack) {
                self.end_attack(attacker_id, changed);
            }
        } else if let Some(building) = self.building(target) {
            let armour = super::table::age_armour(building.kind, self.is_feudal(building.player));
            let Some(building) = self.economy.buildings.get_mut(&target) else {
                return;
            };
            let damage = damage_dealt(attack, kind, armour.melee_armour, armour.pierce_armour);
            building.hp = building.hp.saturating_sub(damage);
            building.last_hit = Some(hit);
            if building.hp == 0 {
                self.destroy_building(target);
                self.end_attack(attacker_id, changed);
            }
        }
    }

    /// Only an idle, living unit without an attack order fights back.
    fn can_retaliate(&self, index: usize) -> bool {
        let unit = &self.units[index].state;
        unit.hp > 0
            && !unit.moving
            && !unit.planning
            && matches!(unit.task, UnitTask::Idle | UnitTask::Carrying)
            && !self.combat.attackers.contains_key(&unit.id)
            && !self.economy.gatherers.contains_key(&unit.id)
            && !self.economy.builders.contains_key(&unit.id)
            && unit_combat(unit.kind).attack > 0
    }
}
