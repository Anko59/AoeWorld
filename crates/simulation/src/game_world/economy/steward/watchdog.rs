//! Managed workers must never sit in a "Moving" task without moving. A stall
//! is judged from position progress alone, so it holds whatever the planner
//! or the collision layer is doing; the cure is an ordinary Stop followed by a
//! fresh job that avoids the node (and, when repeated, the resource) that
//! failed. Core movement and pathfinding are untouched.
use super::snapshot::Snapshot;
use crate::{GameWorld, UnitTask};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_map::ResourceKind;

/// Seconds without position progress, while Moving, before a re-task.
pub const STEWARD_STALL_SECONDS: u64 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Stall {
    pub(crate) at: WorldPosition,
    pub(crate) since: u64,
    pub(crate) strikes: u8,
    pub(crate) avoid_tile: Option<TileCoord>,
    pub(crate) avoid_kind: Option<ResourceKind>,
}

impl GameWorld {
    /// Visits only this domain's managed workers (at most 100) per pass.
    pub(super) fn steward_watchdog(&mut self, s: &Snapshot, changed: &mut Vec<EntityId>) {
        let now = self.tick.0;
        let limit = STEWARD_STALL_SECONDS * u64::from(self.config.tick_hz);
        let workers: Vec<_> = self.economy.steward_homes[&s.steward]
            .workers
            .iter()
            .copied()
            .collect();
        // A stalled worker may be momentarily unmanaged (released, then
        // re-adopted); keep its record until it expires or the unit is gone.
        let alive: Vec<_> = self.economy.steward_homes[&s.steward]
            .stalls
            .keys()
            .copied()
            .filter(|id| self.unit(*id).is_some_and(|u| u.hp > 0))
            .collect();
        if let Some(home) = self.economy.steward_homes.get_mut(&s.steward) {
            home.stalls
                .retain(|id, st| alive.contains(id) && now.saturating_sub(st.since) < limit * 16);
        }
        // Villagers nobody manages are watched too. A "Moving" villager that
        // never moves is not idle to anyone else, so without this it would sit
        // frozen in the territory forever instead of being re-tasked.
        let strays: Vec<_> = s
            .units
            .iter()
            .filter(|u| {
                u.task == UnitTask::Moving
                    // A player's order (kept as manual control, a route being
                    // planned, or a live movement order) is theirs to steer.
                    && !self.economy.stewards.manual.contains(&u.id)
                    && !u.planning
                    && self.movement_order(u.id).is_none()
                    && !self.economy.garrison_orders.contains_key(&u.id)
                    && !workers.contains(&u.id)
            })
            .map(|u| u.id)
            .collect();
        for worker in workers.into_iter().chain(strays) {
            let Some(unit) = self.unit(worker) else {
                continue;
            };
            let stalls = &self.economy.steward_homes[&s.steward].stalls;
            let previous = stalls.get(&worker).copied();
            if unit.task != UnitTask::Moving {
                // A stalled worker keeps its record (so strikes accumulate and the
                // job pass can avoid the failed node) until it has worked on
                // elsewhere for a while.
                if previous.is_some_and(|p| now.saturating_sub(p.since) >= limit * 16)
                    && let Some(home) = self.economy.steward_homes.get_mut(&s.steward)
                {
                    home.stalls.remove(&worker);
                }
                continue;
            }
            let entry = match previous {
                Some(p) if p.at == unit.position => p,
                Some(p) => Stall {
                    at: unit.position,
                    since: now,
                    ..p
                },
                None => Stall {
                    at: unit.position,
                    since: now,
                    strikes: 0,
                    avoid_tile: None,
                    avoid_kind: None,
                },
            };
            let mut entry = entry;
            if now.saturating_sub(entry.since) >= limit {
                let gathering = self
                    .economy
                    .gatherers
                    .get(&worker)
                    .map(|t| (t.tile, t.farm.is_none().then_some(t.kind)));
                entry.strikes = entry.strikes.saturating_add(1);
                entry.avoid_tile = gathering.map(|g| g.0).or(entry.avoid_tile);
                // A failed farm approach excludes that plot, not every Food job.
                // Other paid farms can remain reachable around a Town Center.
                // Repeated natural-node failures still give up the resource kind.
                entry.avoid_kind = if entry.strikes >= 2 {
                    gathering.and_then(|g| g.1).or(entry.avoid_kind)
                } else {
                    entry.avoid_kind
                };
                entry.since = now;
                if self.issue_stop(s.player, worker).is_ok() {
                    changed.push(worker);
                }
            }
            if let Some(home) = self.economy.steward_homes.get_mut(&s.steward) {
                home.stalls.insert(worker, entry);
            }
        }
    }
}

/// Test hook: a stall record with the given avoidance.
#[cfg(test)]
pub(crate) fn steward_stall(
    at: WorldPosition,
    since: u64,
    strikes: u8,
    avoid_tile: Option<TileCoord>,
    avoid_kind: Option<ResourceKind>,
) -> Stall {
    Stall {
        at,
        since,
        strikes,
        avoid_tile,
        avoid_kind,
    }
}

#[cfg(test)]
impl GameWorld {
    pub(crate) fn gather_tile(&self, id: EntityId) -> Option<TileCoord> {
        self.economy.gatherers.get(&id).map(|t| t.tile)
    }
}
