//! Per-tick attack resolution for units that hold an attack order.
use super::{Attacker, HitRecord, MELEE_RANGE_SUBUNITS, damage_dealt, unit_combat};
use crate::game_world::facing_for;
use crate::{GameWorld, UnitTask};
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
    pub(super) fn centre(&self) -> WorldPosition {
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

    pub(crate) fn advance_combat(&mut self, changed: &mut Vec<EntityId>) {
        self.combat.stats = Default::default();
        self.combat.far_searches = super::approach::MAX_FAR_SEARCHES_PER_TICK;
        self.advance_projectiles(changed);
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

    fn step_attacker(&mut self, id: EntityId, now: u64, changed: &mut Vec<EntityId>) {
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
        if unit_combat(before.kind).attack_kind == super::DamageKind::Pierce
            && !self.target_visible_to(before.player, attacker.target)
        {
            self.end_attack(id, changed);
            return;
        }
        let stats = self.unit_stats(id).unwrap_or(*unit_combat(before.kind));
        let range = i64::from(stats.range) * MELEE_RANGE_SUBUNITS;
        let distance = goal.distance_squared(before.position);
        if distance > range * range {
            self.approach(id, index, attacker, goal, now, changed);
            return;
        }
        let min = i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS;
        if distance < min * min {
            self.stop_movement(index);
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
            changed.push(self.units[index].state.id);
        }
    }

    fn approach(
        &mut self,
        id: EntityId,
        index: usize,
        attacker: Attacker,
        goal: Goal,
        now: u64,
        changed: &mut Vec<EntityId>,
    ) {
        let before = self.units[index].state;
        let order = self.units[index].order;
        if order.is_none() && self.units[index].last_movement_error.is_some() {
            self.end_attack(id, changed);
            return;
        }
        // The firing-position search runs only when the repath timer expires.
        if now >= attacker.repath_tick {
            let next_repath = now.saturating_add(self.repath_ticks());
            if let Some(entry) = self.combat.attackers.get_mut(&id) {
                entry.repath_tick = next_repath;
            }
            let centre = goal.centre().tile_floor();
            let from = self.units[index].state.position.tile_floor();
            let reusable = attacker.far.filter(|far| {
                far.centre == centre
                    && i64::from((from.x - centre.x).abs().max((from.y - centre.y).abs()))
                        > super::approach::FAR_REUSE_TILES
            });
            let destination = if let Some(far) = reusable {
                Some(far.destination)
            } else if attacker.far.is_some() && self.combat.far_searches == 0 {
                // Over this tick's budget: keep walking and look again next tick.
                if let Some(entry) = self.combat.attackers.get_mut(&id) {
                    entry.repath_tick = now.saturating_add(1);
                }
                self.units[index].state.task = UnitTask::Attacking;
                if self.units[index].state != before {
                    changed.push(self.units[index].state.id);
                }
                return;
            } else {
                let search = self.attack_search(id, &goal).ok();
                if attacker.far.is_some() {
                    self.combat.far_searches -= 1;
                }
                let far = search
                    .as_ref()
                    .filter(|search| search.far)
                    .and_then(|search| search.destination)
                    .map(|destination| super::approach::FarAttack {
                        centre,
                        destination,
                    });
                if let Some(entry) = self.combat.attackers.get_mut(&id) {
                    entry.far = far;
                }
                search.and_then(|search| search.destination)
            };
            let stale = match (order, destination) {
                (None, _) => true,
                (Some(order), Some(at)) => order.target_tile != at.tile_floor(),
                (Some(_), None) => true,
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
            changed.push(self.units[index].state.id);
        }
    }

    fn repath_attacker(
        &mut self,
        id: EntityId,
        destination: Option<WorldPosition>,
        now: u64,
        changed: &mut Vec<EntityId>,
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
    pub(crate) fn end_attack(&mut self, id: EntityId, changed: &mut Vec<EntityId>) {
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
        changed.push(self.units[index].state.id);
    }

    fn strike(&mut self, attacker_id: EntityId, target: EntityId, changed: &mut Vec<EntityId>) {
        let Some(&attacker_index) = self.lookup.get(&attacker_id) else {
            return;
        };
        let stats = self
            .unit_stats(attacker_id)
            .unwrap_or(*unit_combat(self.units[attacker_index].state.kind));
        let bonus = self
            .unit(target)
            .map_or(0, |u| super::military::class_bonus(stats.kind, u.kind));
        if stats.attack_kind == super::DamageKind::Pierce {
            let kind = if stats.kind == crate::UnitKind::Skirmisher {
                super::ProjectileKind::Javelin
            } else {
                super::ProjectileKind::Arrow
            };
            self.launch_projectile(attacker_id, target, stats.attack, bonus, kind);
            return;
        }
        self.resolve_hit(
            attacker_id,
            target,
            stats.attack,
            bonus,
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
        bonus: u16,
        kind: super::DamageKind,
        changed: &mut Vec<EntityId>,
    ) {
        let hit = HitRecord {
            attacker: attacker_id,
            tick: Tick(self.tick.0),
        };
        self.combat.stats.hits += 1;
        if let Some(&victim_index) = self.lookup.get(&target) {
            if self.units[victim_index].state.hp == 0 {
                return;
            }
            let (melee, pierce) = self.unit_armour(target).unwrap_or_default();
            let victim = &mut self.units[victim_index].state;
            let damage = super::table::damage_with_bonus(attack, kind, melee, pierce, bonus);
            victim.hp = victim.hp.saturating_sub(damage);
            victim.last_hit = Some(hit);
            let (hp, victim_id) = (victim.hp, victim.id);
            if hp == 0 {
                self.kill_unit(victim_index);
                self.finish_target_attack(attacker_id, target, changed);
            } else if self.can_retaliate(victim_index) {
                self.begin_attack(victim_id, attacker_id);
            }
            changed.push(self.units[victim_index].state.id);
        } else if self.animal(target).is_some() {
            if self.strike_animal(target, attacker_id, attack) {
                self.finish_target_attack(attacker_id, target, changed);
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
                self.finish_target_attack(attacker_id, target, changed);
            }
        }
    }

    fn finish_target_attack(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        changed: &mut Vec<EntityId>,
    ) {
        if self
            .combat
            .attackers
            .get(&attacker)
            .is_some_and(|a| a.target == target)
        {
            self.end_attack(attacker, changed);
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
