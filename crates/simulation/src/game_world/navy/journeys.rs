//! Shore rendezvous retain ordinary unit movement and its deterministic budgets.
use super::*;
use crate::{GameWorldError, Terrain};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};

type Shore = (TileCoord, TileCoord);
type SearchNode = (TileCoord, bool);
type ShoreRecord = (u32, Option<Shore>);

#[derive(Clone, Debug)]
pub(crate) struct Journey {
    pub player: PlayerId,
    pub passengers: Vec<EntityId>,
    pub land: TileCoord,
    pub water: TileCoord,
    pub unloading: bool,
}

impl GameWorld {
    /// A shortest combined land/water approach, with one boarding transition.
    /// The 4,096-record cap bounds discovery; actual actors use ordinary routes.
    pub(super) fn boarding_shore(&self, land: TileCoord, ship: TileCoord) -> Option<Shore> {
        self.find_transport_shore(land, ship, true)
    }

    fn dry_shore(&self, tile: TileCoord) -> bool {
        matches!(&self.terrain, Terrain::Map { generator, .. }
            if generator.tile_at(tile).is_some_and(|t| t.water == aoe_map::WaterKind::None && t.passable))
    }

    fn find_transport_shore(&self, land: TileCoord, ship: TileCoord, walk: bool) -> Option<Shore> {
        let distance = |at: TileCoord| {
            let dx = at.x.abs_diff(ship.x);
            let dy = at.y.abs_diff(ship.y);
            10 * dx.max(dy) + 4 * dx.min(dy)
        };
        let mut records: BTreeMap<SearchNode, ShoreRecord> =
            BTreeMap::from([((land, false), (0, None))]);
        let mut open = BinaryHeap::from([Reverse((distance(land), 0_u32, land.y, land.x, false))]);
        while let Some(Reverse((_, cost, y, x, naval))) = open.pop() {
            let at = TileCoord::new(x, y);
            let &(known, shore) = records.get(&(at, naval))?;
            if cost != known {
                continue;
            }
            if naval && at == ship {
                return shore;
            }
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
                let next = TileCoord::new(x + dx, y + dy);
                let next_cost = cost + if dx == 0 || dy == 0 { 10 } else { 14 };
                let land_step =
                    !naval && walk && self.unit_step_clear(UnitKind::Villager, at, next).ok()?;
                let boat_step = if naval {
                    self.unit_step_clear(UnitKind::TransportShip, at, next)
                        .ok()?
                } else {
                    self.dry_shore(at)
                        && self
                            .unit_tile_passable(UnitKind::TransportShip, next, &|| false)
                            .ok()?
                };
                for domain in [false, true] {
                    if !(if domain { boat_step } else { land_step }) {
                        continue;
                    }
                    let key = (next, domain);
                    if records.get(&key).is_some_and(|r| r.0 <= next_cost)
                        || (records.len() >= 4_096 && !records.contains_key(&key))
                    {
                        continue;
                    }
                    let next_shore = if domain && !naval {
                        Some((at, next))
                    } else {
                        shore
                    };
                    records.insert(key, (next_cost, next_shore));
                    open.push(Reverse((
                        next_cost + distance(next),
                        next_cost,
                        next.y,
                        next.x,
                        domain,
                    )));
                }
            }
        }
        None
    }

    pub(super) fn unloading_shore(&self, land: TileCoord, ship: TileCoord) -> Option<TileCoord> {
        if !self.dry_shore(land)
            || !self
                .unit_tile_passable(UnitKind::Villager, land, &|| false)
                .ok()?
        {
            return None;
        }
        if adjacent(land, ship) {
            return Some(ship);
        }
        self.find_transport_shore(land, ship, false)
            .map(|(_, water)| water)
    }

    pub(crate) fn cancel_transport_journey(&mut self, id: EntityId) {
        self.economy
            .transport_journeys
            .retain(|ship, job| *ship != id && !job.passengers.contains(&id));
    }

    pub(super) fn queue_transport_journey(&mut self, ship: EntityId, job: Journey) {
        for id in job.passengers.iter().copied().chain(std::iter::once(ship)) {
            let index = self.lookup[&id];
            self.stop_unit_orders(index);
            self.cancel_hunt(id);
        }
        self.economy.transport_journeys.insert(ship, job);
    }

    pub(crate) fn advance_transports(&mut self, changed: &mut Vec<EntityId>) {
        let ships = self
            .economy
            .transport_journeys
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for ship in ships {
            let Some(mut job) = self.economy.transport_journeys.remove(&ship) else {
                continue;
            };
            let Ok(water) = self.validate_transport(job.player, ship) else {
                continue;
            };
            let alive = job.passengers.iter().all(|id| {
                self.unit(*id)
                    .is_some_and(|u| u.hp > 0 && u.player == job.player)
                    && !self.embarked(*id)
            });
            if !job.unloading && !alive {
                continue;
            }
            // Board arrivals as they reach the bank. Physical avoidance keeps
            // a group from stacking on one rendezvous tile; each boarding frees
            // that approach for the passengers still on their way.
            if water == job.water && !job.unloading {
                let ready = job
                    .passengers
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.unit(*id)
                            .is_some_and(|u| adjacent(u.position.tile_floor(), water))
                    })
                    .collect::<Vec<_>>();
                if !ready.is_empty() && self.issue_load(job.player, &ready, ship).is_ok() {
                    changed.push(ship);
                    changed.extend(&ready);
                    job.passengers.retain(|id| !ready.contains(id));
                    if job.passengers.is_empty() {
                        continue;
                    }
                }
            }
            if water == job.water
                && (job.unloading
                    || job.passengers.iter().all(|id| {
                        self.unit(*id)
                            .is_some_and(|u| adjacent(u.position.tile_floor(), water))
                    }))
            {
                let cargo = self.transport_cargo(ship).to_vec();
                let result = if job.unloading {
                    self.issue_unload(job.player, ship, job.land)
                } else {
                    self.issue_load(job.player, &job.passengers, ship)
                };
                if result.is_ok() {
                    changed.push(ship);
                    changed.extend(job.passengers);
                    changed.extend(cargo);
                }
                continue;
            }
            let mut failed = false;
            for (id, target) in std::iter::once((ship, job.water)).chain(
                job.passengers
                    .iter()
                    .map(|id| (*id, job.land))
                    .filter(|_| !job.unloading),
            ) {
                let Some(u) = self.unit(id) else {
                    failed = true;
                    break;
                };
                if u.position.tile_floor() != target && !u.moving && !u.planning {
                    let result = WorldPosition::from_tile_center(target)
                        .map_err(|_| GameWorldError::InvalidPosition)
                        .and_then(|at| self.issue_task_move(id, at));
                    match result {
                        Ok(_) => {
                            self.units[self.lookup[&id]].state.task = UnitTask::Moving;
                            changed.push(id);
                        }
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
            }
            if !failed {
                self.economy.transport_journeys.insert(ship, job);
            }
        }
    }
}
