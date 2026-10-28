//! Per-tick attack resolution for units that hold an attack order.
use super::{Attacker, HitRecord, MELEE_RANGE_SUBUNITS, unit_combat};
use crate::{GameWorld, UnitTask};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, Tick, TileRect, WorldPosition};

/// What an attacker is walking to and hitting.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Goal {
    Unit(WorldPosition, i32),
    Building(TileRect),
}

impl Goal {
    pub(crate) fn distance_squared(&self, from: WorldPosition) -> i64 {
        let (x, y) = (i64::from(from.x), i64::from(from.y));
        let (cx, cy) = match *self {
            Goal::Unit(at, _) => (i64::from(at.x), i64::from(at.y)),
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
    pub(in crate::game_world) fn centre(&self) -> WorldPosition {
        match *self {
            Goal::Unit(at, _) => at,
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
            return (unit.hp > 0 && unit.player != attacker && !self.embarked(target))
                .then_some(Goal::Unit(unit.position, unit.kind.collision_radius()));
        }
        if let Some(animal) = self.animal(target) {
            return (animal.hp > 0).then_some(Goal::Unit(animal.position, 205));
        }
        self.economy
            .buildings
            .get(&target)
            .filter(|building| building.hp > 0 && building.player != attacker)
            .map(|building| Goal::Building(building.footprint))
    }

    pub(crate) fn advance_combat(&mut self, changed: &mut Vec<EntityId>) {
        self.combat.stats = Default::default();
        self.advance_projectiles(changed);
        let now = self.tick.0;
        while let Some(&(due, id)) = self.combat.dying.first() {
            if due > now {
                break;
            }
            self.combat.dying.pop_first();
            self.remove_unit(id);
        }
        self.advance_acquisition(changed);
        // Melee retaliation created during this loop acts next tick;
        // projectile reactions and acquisition already join this snapshot.
        let ids = self.combat.attackers.keys().copied().collect::<Vec<_>>();
        self.combat.pursuit_floor = self.combat.pursuit_cursor;
        self.combat.pursuit_cursor = None;
        for id in ids {
            self.step_attacker(id, now, changed);
        }
        if self.combat.searches_used < super::approach::MAX_ATTACK_SEARCHES_PER_TICK
            && self.combat.position_searches > 0
        {
            self.combat.pursuit_cursor = None;
        }
        self.combat.pursuit_floor = None;
        self.advance_towers(changed);
        self.advance_healing(changed);
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
        if !self.pursuit_allowed(id, goal, changed) {
            return;
        }
        if (unit_combat(before.kind).attack_kind == super::DamageKind::Pierce
            || before.kind == crate::UnitKind::Mangonel)
            && !self.target_visible_to(before.player, attacker.target)
        {
            self.end_attack(id, changed);
            return;
        }
        let stats = self.unit_stats(id).unwrap_or(*unit_combat(before.kind));
        let range = super::super::navy::attack_range(&stats);
        let distance = goal.distance_squared(before.position);
        let approaching_slot = Self::uses_melee_position(before.kind)
            && (!self.arrival_position_free(index)
                || self.units[index].order.is_some_and(|o| {
                    super::super::movement_step::slots::squared(before.position, o.destination)
                        > 64 * 64
                }));
        if distance > range * range || approaching_slot {
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
        let state = &mut self.units[index].state;
        match goal {
            Goal::Unit(target, _) => state.face_towards(target),
            Goal::Building(footprint) => state.face_footprint(footprint),
        }
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
        // The firing-position search runs only when the repath timer expires,
        // and never while the current route is still being searched: a new
        // order would restart that search, and a chase whose search needs
        // longer than the repath interval would never move at all.
        if now >= attacker.repath_tick && !self.waiting_for_route(index) {
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
            // A search for a target beyond the local search's reach ends in the
            // far fallback, first search or repeat; both spend the budget.
            let counts_as_far = attacker.far.is_some()
                || i64::from((from.x - centre.x).abs().max((from.y - centre.y).abs()))
                    > super::approach::FAR_REUSE_TILES;
            // An unchanged nearby target keeps its route, unless a melee
            // attacker's reserved position has since been taken.
            let retained = order.filter(|o| {
                !counts_as_far
                    && attacker.goal_tile == Some(centre)
                    && (!Self::uses_melee_position(before.kind)
                        || self.interaction_point_free(id, o.destination))
            });
            let destination = if let Some(order) = retained {
                Some(order.destination)
            } else if let Some(far) = reusable {
                Some(far.destination)
            } else if (counts_as_far && self.combat.position_searches == 0)
                || !self.reserve_pursuit_search(id)
            {
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
                if let Some(wait) = search.as_ref().and_then(|s| s.wait) {
                    if wait == super::SearchWait::Budget
                        && let Some(entry) = self.combat.attackers.get_mut(&id)
                    {
                        entry.repath_tick = now.saturating_add(1);
                    }
                    // Preserve the order and job. Crowding or a spent budget
                    // is not proof that the target is unreachable.
                    self.units[index].state.task = UnitTask::Attacking;
                    if self.units[index].state != before {
                        changed.push(id);
                    }
                    return;
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
                    entry.goal_tile = Some(centre);
                }
                search.and_then(|search| search.destination)
            };
            // A far chase follows its target loosely: it is re-planned once
            // the target has moved more than a quarter of the remaining way.
            // Re-planning every attacker each second toward a target a few
            // tiles off was most of the route demand of a 99-AI match.
            let here = self.units[index].state.position.tile_floor();
            let stale = match (order, destination) {
                (None, _) => true,
                (Some(order), Some(at)) => {
                    let to = at.tile_floor();
                    let moved = (order.target_tile.x - to.x)
                        .abs()
                        .max((order.target_tile.y - to.y).abs());
                    let remaining = (here.x - to.x).abs().max((here.y - to.y).abs());
                    moved > (remaining / 4).max(0)
                        || (remaining <= 4
                            && super::super::movement_step::slots::squared(order.destination, at)
                                > 128 * 128)
                }
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
            .map_or(0, |u| super::military::class_bonus(stats.kind, u.kind))
            + self.naval_damage_bonus(stats.kind, target);
        if stats.kind == crate::UnitKind::DemolitionShip {
            self.detonate(attacker_id, target, changed);
            return;
        }
        if stats.kind == crate::UnitKind::FireShip {
            self.resolve_hit(
                (attacker_id, self.units[attacker_index].state.player),
                target,
                stats.attack,
                bonus,
                stats.attack_kind,
                changed,
            );
            return;
        }
        if stats.kind == crate::UnitKind::Mangonel {
            self.launch_projectile(
                attacker_id,
                target,
                stats.attack,
                bonus,
                super::ProjectileKind::Stone,
            );
            return;
        }
        if stats.attack_kind == super::DamageKind::Pierce {
            let kind = if matches!(
                stats.kind,
                crate::UnitKind::Skirmisher | crate::UnitKind::EliteSkirmisher
            ) {
                super::ProjectileKind::Javelin
            } else {
                super::ProjectileKind::Arrow
            };
            self.launch_projectile(attacker_id, target, stats.attack, bonus, kind);
            return;
        }
        self.resolve_hit(
            (attacker_id, self.units[attacker_index].state.player),
            target,
            stats.attack,
            bonus,
            stats.attack_kind,
            changed,
        );
    }

    /// Both melee and tower arrows share armour, death and retaliation logic.
    pub(in crate::game_world) fn resolve_hit(
        &mut self,
        source: (EntityId, PlayerId),
        target: EntityId,
        attack: u16,
        bonus: u16,
        kind: super::DamageKind,
        changed: &mut Vec<EntityId>,
    ) {
        let (attacker_id, attacker_player) = source;
        let hit = HitRecord {
            attacker: attacker_id,
            tick: Tick(self.tick.0),
        };
        self.combat.stats.hits += 1;
        if let Some(&victim_index) = self.lookup.get(&target) {
            if self.units[victim_index].state.hp == 0 || self.embarked(target) {
                return;
            }
            let (melee, pierce) = self.unit_armour(target).unwrap_or_default();
            let victim = &mut self.units[victim_index].state;
            let damage = super::table::damage_with_bonus(attack, kind, melee, pierce, bonus);
            victim.hp = victim.hp.saturating_sub(damage);
            victim.last_hit = Some(hit);
            let (hp, victim_player) = (victim.hp, victim.player);
            self.record_combat_hit(
                Some(attacker_player),
                victim_player,
                kind == super::DamageKind::Pierce,
                hp == 0,
            );
            if hp == 0 {
                self.kill_unit(victim_index);
                self.finish_target_attack(attacker_id, target, changed);
            } else if attacker_player != victim_player {
                self.retaliate(victim_index, attacker_id, changed);
            }
            changed.push(self.units[victim_index].state.id);
        } else if self.animal(target).is_some() {
            if self.strike_animal(target, attacker_id, attack) {
                self.finish_target_attack(attacker_id, target, changed);
            }
        } else if let Some(building) = self.building(target) {
            let armour = super::table::age_armour(building.kind, building.age);
            let masonry = self.has_researched(building.player, crate::Tech::Masonry)
                && !matches!(
                    building.kind,
                    crate::BuildingKind::Farm
                        | crate::BuildingKind::PalisadeWall
                        | crate::BuildingKind::PalisadeGate
                );
            let class_armour = if building.kind == crate::BuildingKind::Castle {
                8
            } else {
                0
            } + if masonry { 3 } else { 0 };
            let building_bonus =
                self.unit(attacker_id)
                    .map_or(bonus.saturating_sub(class_armour), |u| match u.kind {
                        crate::UnitKind::BatteringRam => 150_u16.saturating_sub(class_armour),
                        crate::UnitKind::Mangonel => bonus.saturating_sub(class_armour),
                        // Standard-building class 21 is distinct from the siege
                        // building class 11 armour improved by Masonry.
                        crate::UnitKind::LongSwordsman => 3,
                        crate::UnitKind::Pikeman => 1,
                        _ => bonus,
                    });
            let damage = super::table::damage_with_bonus(
                attack,
                kind,
                armour.melee_armour + i16::from(masonry),
                armour.pierce_armour + u16::from(masonry),
                building_bonus,
            );
            let Some(building) = self.economy.buildings.get_mut(&target) else {
                return;
            };
            building.hp = building.hp.saturating_sub(damage);
            building.last_hit = Some(hit);
            let occupants: Vec<_> = building.garrison.iter().map(|g| g.unit.id).collect();
            let (building_kind, footprint) = (building.kind, building.footprint);
            let (player, killed) = (building.player, building.hp == 0);
            self.record_combat_hit(
                Some(attacker_player),
                player,
                kind == super::DamageKind::Pierce,
                killed,
            );
            if killed {
                // Ejection restores map units during this hit. Include their
                // identities in the ordinary delta for simulation consumers.
                changed.extend(occupants);
                if building_kind == crate::BuildingKind::Bridge {
                    let rect = footprint;
                    changed.extend(self.query(rect).0.into_iter().map(|u| u.id));
                }
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
}
