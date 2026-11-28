//! The observer of a run: after every tick it reads the structures the engine
//! overwrites at the next tick (combat, the Steward's and the General's work,
//! the deliveries window) and watches every unit's movement order; at sampled
//! ticks it takes the census and the cumulative counters. It only reads, so a
//! run's play is the same with or without it. The windows are summed from what
//! it keeps (`windows`).
mod arrival;
mod income;
mod row;
mod sample;
#[cfg(test)]
mod tests;

pub(super) use row::{Delay, Events, GeneralTotals, StewardTotals, TickRow};
pub(super) use sample::{ISSUERS, IssuerTotals, RouteTotals, Sample};

use super::choke::{Choke, ChokeWatch, Moves, Walker};
use aoe_core::PlayerId;
use aoe_simulation::GameWorld;
use arrival::{Orders, Seen};
pub(super) use income::Income;
use std::time::Duration;

pub(super) struct Recorder {
    seats: usize,
    orders: Orders,
    income: Income,
    /// The highest unit and building id seen: ids only grow, so an id above
    /// it is a unit or building that did not exist before.
    max_unit: u32,
    max_building: u32,
    buildings_born: u64,
    /// The watch at the choke of a staged scenario, and the units that fought for the first time
    /// since the last row (told by `blows`, which the driver calls after each tick).
    choke: Option<ChokeWatch>,
    fighters_new: u64,
    /// One row per played tick: `rows[n - 1]` is tick `n`.
    pub rows: Vec<TickRow>,
    pub samples: Vec<Sample>,
    pub events: Events,
}

impl Recorder {
    pub fn new(seats: usize, tick_hz: u32) -> Self {
        Self {
            seats,
            orders: Orders::default(),
            income: Income::new(tick_hz),
            max_unit: 0,
            max_building: 0,
            buildings_born: 0,
            choke: None,
            fighters_new: 0,
            rows: Vec::new(),
            samples: Vec::new(),
            events: Events::default(),
        }
    }

    /// Watches `choke` as well: crossings, standing still in the neck, who fought. Call it before
    /// `start`.
    pub fn stage(&mut self, choke: &Choke) {
        self.choke = Some(ChokeWatch::new(choke));
    }

    /// The hits of the tick just played, as (attacker, target) pairs, for the next row. The
    /// engine keeps them only on request (`set_damage_observation`), so only a staged scenario
    /// has them.
    pub fn blows(&mut self, hits: impl IntoIterator<Item = (u32, u32)>) {
        if let Some(watch) = &mut self.choke {
            self.fighters_new += watch.blows(hits);
        }
    }

    /// The match as it starts, before its first tick: the orders already given
    /// and the highest ids are known, and the first sample is taken.
    pub fn start(&mut self, world: &GameWorld) {
        let _ = self.read(world);
        self.count_new_buildings(world);
        self.buildings_born = 0;
        self.sample(world);
    }

    /// After a played tick that took `spent` inside `tick()`.
    pub fn tick(&mut self, world: &GameWorld, spent: Duration) {
        let mut row = self.read(world);
        row.nanos = u64::try_from(spent.as_nanos()).unwrap_or(u64::MAX);
        self.rows.push(row);
    }

    /// The census and the cumulative counters now.
    pub fn sample(&mut self, world: &GameWorld) {
        self.buildings_born += self.count_new_buildings(world);
        let mut sample = sample::capture(world, self.seats, self.buildings_born);
        sample.choke = self.choke.as_ref().map(ChokeWatch::totals);
        self.samples.push(sample);
    }

    fn read(&mut self, world: &GameWorld) -> TickRow {
        let tick = world.tick().0;
        let mut row = TickRow::default();
        let combat = world.combat_stats();
        row.hits = u64::from(combat.hits);
        row.attackers = u64::from(combat.attackers);
        row.attack_searches = u64::from(combat.attack_searches);
        row.first_blows = u64::from(combat.first_hits);
        row.first_blow_wait_ticks = combat.first_hit_wait_ticks;
        row.steward = world.steward_work().into();
        row.general = world.general_work().into();
        let mut recent = [0_u64; 4];
        for seat in 0..self.seats {
            for (total, amount) in recent
                .iter_mut()
                .zip(world.recent_deliveries(PlayerId(seat as u16)))
            {
                *total += amount;
            }
        }
        let (credits, fault) = self.income.credits(tick, recent);
        row.delivered = credits;
        row.income_faults = u64::from(fault);
        let mut highest = self.max_unit;
        let mut moves = Moves::default();
        for unit in world.units() {
            if unit.id.0 > self.max_unit {
                row.units_born += 1;
                highest = highest.max(unit.id.0);
            }
            let goal = (unit.hp > 0)
                .then(|| world.movement_order(unit.id))
                .flatten()
                .map(|order| (order.destination.x, order.destination.y));
            if let Some(watch) = &mut self.choke {
                // A soldier in a formation arrives at the slot it was assigned, not at the
                // destination of the army (twelve tiles wide): the slot is its goal.
                let slot = (unit.hp > 0)
                    .then(|| world.formation_destination_slot(unit.id))
                    .flatten()
                    .map(|slot| (slot.x, slot.y));
                watch.see(
                    Walker {
                        id: unit.id.0,
                        seat: unit.player.0,
                        position: (unit.position.x, unit.position.y),
                        stepped: unit.position != unit.previous_position,
                        goal: slot.or(goal),
                        alive: unit.hp > 0,
                    },
                    &mut moves,
                );
            }
            if unit.hp == 0 {
                continue;
            }
            let seen = Seen {
                id: unit.id.0,
                position: (unit.position.x, unit.position.y),
                stepped: unit.position != unit.previous_position,
                goal,
            };
            self.orders.watch(tick, seen, &mut row, &mut self.events);
        }
        self.max_unit = highest;
        self.orders.finish(tick, &mut row, &mut self.events);
        row.crossed = moves.crossed;
        row.still_inside_peak = moves.still_peak;
        row.alive_by_seat = moves.alive;
        row.fighters_new = std::mem::take(&mut self.fighters_new);
        row
    }

    fn count_new_buildings(&mut self, world: &GameWorld) -> u64 {
        let (mut born, mut highest) = (0, self.max_building);
        for building in world.buildings() {
            if building.id.0 > self.max_building {
                born += 1;
                highest = highest.max(building.id.0);
            }
        }
        self.max_building = highest;
        born
    }
}
