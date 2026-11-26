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

use aoe_core::PlayerId;
use aoe_simulation::GameWorld;
use arrival::{Orders, Seen};
use income::Income;
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
            rows: Vec::new(),
            samples: Vec::new(),
            events: Events::default(),
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
        self.samples
            .push(sample::capture(world, self.seats, self.buildings_born));
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
        for unit in world.units() {
            if unit.id.0 > self.max_unit {
                row.units_born += 1;
                highest = highest.max(unit.id.0);
            }
            if unit.hp == 0 {
                continue;
            }
            let goal = world
                .movement_order(unit.id)
                .map(|order| (order.destination.x, order.destination.y));
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
