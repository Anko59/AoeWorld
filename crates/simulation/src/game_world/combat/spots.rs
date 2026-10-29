//! Melee endpoints are free circles on the target perimeter, reached via the
//! ordinary static attack search. Ranged searches retain their range boundary.
use super::tick::Goal;
use crate::game_world::movement_step::slots::{perimeter_positions, squared};
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, WorldPosition};

impl GameWorld {
    pub(super) fn can_strike_on_approach(
        &self,
        id: EntityId,
        goal: &Goal,
        distance: i64,
        range: i64,
    ) -> bool {
        distance <= range * range
            && matches!(goal, Goal::Unit(..))
            && self.combat.attackers.get(&id).is_some_and(|a| {
                a.origin.is_some() || a.resume.is_some() || self.crowded_melee_contact(id, goal)
            })
    }

    /// Reuse contact pressure only while this mover owns the sequential turn.
    pub(crate) fn melee_contact_traffic(&self, id: EntityId, goal: &Goal) -> bool {
        if let Some((cached, at, clear)) = self.melee_traffic_clearance
            && cached == id
            && self
                .lookup
                .get(&id)
                .is_some_and(|&index| self.units[index].state.position == at)
        {
            return clear;
        }
        self.crowded_melee_contact(id, goal)
    }

    pub(crate) fn crowded_melee_contact(&self, id: EntityId, goal: &Goal) -> bool {
        let Goal::Unit(_, target_radius) = *goal else {
            return false;
        };
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        let kind = self.units[index].state.kind;
        if kind.is_naval() || !Self::uses_melee_position(kind) {
            return false;
        }
        // Integer circumference (~6r) divided by one full body diameter.
        let radius = kind.collision_radius();
        let capacity = ((radius + target_radius) * 6 / (2 * radius)).max(2) as usize;
        self.friendly_contact_pressure(index, capacity)
    }

    pub(super) fn approaching_melee_slot(&self, index: usize) -> bool {
        let unit = self.units[index].state;
        Self::uses_melee_position(unit.kind)
            && (self.units[index]
                .order
                .is_some_and(|o| squared(unit.position, o.destination) > 64 * 64)
                || !self.arrival_position_free(index))
    }

    pub(super) fn melee_positions(
        &self,
        unit: &crate::GameUnit,
        range: i64,
        goal: &Goal,
    ) -> Vec<WorldPosition> {
        let id = unit.id;
        let mut points = match goal {
            Goal::Building(rect) => perimeter_positions(*rect, unit.kind),
            Goal::Unit(at, target_radius) => {
                let contact = (unit.kind.collision_radius() + target_radius + 8).min(range as i32);
                let mut points = circle_positions(*at, contact);
                // Contact points guide the final approach, but a free point
                // anywhere in actual reach also admits a crowded fighter.
                if contact < range as i32 && self.crowded_melee_contact(id, goal) {
                    points.extend(circle_positions(*at, range as i32));
                }
                points
            }
        };
        let valid = |at: &WorldPosition| {
            let at = *at;
            goal.distance_squared(at) <= range * range
                && self.config.valid_ground_position(at)
                && !self.economy.blocked_tiles.contains(&at.tile_floor())
                && self
                    .unit_tile_passable(unit.kind, at.tile_floor(), &|| false)
                    .unwrap_or(false)
                && self.body_position_clear(unit.kind, at)
        };
        points.retain(valid);
        // A target can stand inside newly blocked ground. Its contact ring
        // is then obstructed too; use the nearest legal firing reach instead.
        if points.is_empty()
            && let Goal::Unit(at, _) = goal
        {
            points = circle_positions(*at, range as i32);
            points.retain(valid);
        }
        points.sort_unstable_by_key(|&at| (squared(unit.position, at), at.y, at.x));
        points
    }

    /// Whether the attacker could fight `target` now: it is in reach, or it
    /// fights at range, or a melee spot around the target is free. When every
    /// spot is taken an attack would only stand and wait (`SearchWait::Crowded`).
    pub(crate) fn attack_spot_free(&self, id: EntityId, target: EntityId) -> bool {
        let (Some(unit), Some(stats)) = (self.unit(id), self.unit_stats(id)) else {
            return false;
        };
        let Some(goal) = self.attack_goal(target, unit.player) else {
            return false;
        };
        let range = super::super::navy::attack_range(&stats);
        !Self::uses_melee_position(unit.kind)
            || goal.distance_squared(unit.position) <= range * range
            || self.melee_position_free(&unit, range, &goal)
    }

    /// `melee_positions(unit, range, goal).any(interaction_point_free)` without
    /// building or sorting the list: the first free legal spot ends the scan.
    fn melee_position_free(&self, unit: &crate::GameUnit, range: i64, goal: &Goal) -> bool {
        let id = unit.id;
        let valid = |at: WorldPosition| {
            goal.distance_squared(at) <= range * range
                && self.config.valid_ground_position(at)
                && !self.economy.blocked_tiles.contains(&at.tile_floor())
                && self
                    .unit_tile_passable(unit.kind, at.tile_floor(), &|| false)
                    .unwrap_or(false)
                && self.body_position_clear(unit.kind, at)
        };
        // Some(true): a free spot; Some(false): legal spots, all taken.
        let scan = |points: Vec<WorldPosition>| {
            let mut any_valid = false;
            for at in points {
                // A free point is legal by definition (`interaction_point_free`
                // repeats every other legality test), so only a taken one
                // needs the separate legality check.
                if goal.distance_squared(at) <= range * range {
                    if self.interaction_point_free(id, at) {
                        return Some(true);
                    }
                    any_valid = any_valid || valid(at);
                }
            }
            any_valid.then_some(false)
        };
        let ring = match goal {
            Goal::Building(rect) => perimeter_positions(*rect, unit.kind),
            Goal::Unit(at, target_radius) => circle_positions(
                *at,
                (unit.kind.collision_radius() + target_radius + 8).min(range as i32),
            ),
        };
        match (scan(ring), goal) {
            (Some(true), _) => true,
            (Some(false), _) => self.crowded_melee_contact(id, goal),
            (None, Goal::Unit(at, _)) => scan(circle_positions(*at, range as i32))
                .is_some_and(|free| free || self.crowded_melee_contact(id, goal)),
            (None, _) => false,
        }
    }

    /// An attack the unit chose itself (acquisition, or an attack-move
    /// member) gives up a target whose every spot is taken (`wait` is
    /// `Crowded`) while it stands, with no route, out of reach: it looks for another enemy or marches on rather than
    /// wait there. An explicit order keeps waiting.
    pub(super) fn gives_up_ringed_target(
        &self,
        id: EntityId,
        attacker: &super::Attacker,
        goal: Goal,
        wait: Option<super::SearchWait>,
        standing: bool,
    ) -> bool {
        wait == Some(super::SearchWait::Crowded)
            && standing
            && (attacker.origin.is_some() || attacker.resume.is_some())
            && self
                .unit(id)
                .zip(self.unit_stats(id))
                .is_some_and(|(u, stats)| {
                    let range = super::super::navy::attack_range(&stats);
                    goal.distance_squared(u.position) > range * range
                })
    }

    pub(super) fn uses_melee_position(kind: UnitKind) -> bool {
        let stats = super::unit_combat(kind);
        stats.attack_kind == super::DamageKind::Melee && stats.range <= 1
    }
}

fn circle_positions(center: WorldPosition, radius: i32) -> Vec<WorldPosition> {
    const DIRECTIONS: [(i32, i32); 12] = [
        (1024, 0),
        (886, 512),
        (512, 886),
        (0, 1024),
        (-512, 886),
        (-886, 512),
        (-1024, 0),
        (-886, -512),
        (-512, -886),
        (0, -1024),
        (512, -886),
        (886, -512),
    ];
    DIRECTIONS
        .into_iter()
        .map(|(x, y)| {
            WorldPosition::new(center.x + x * radius / 1024, center.y + y * radius / 1024)
        })
        .collect()
}
