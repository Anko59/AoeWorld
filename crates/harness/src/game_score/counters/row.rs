//! What the observer keeps of every tick: the numbers that add up over a
//! window (`TickRow`) and the observations that do not (`Events`).
use aoe_simulation::{GeneralWork, StewardWork};
use serde::{Deserialize, Serialize};
use std::ops::AddAssign;

/// The Steward's work, summed over ticks (`StewardWork` is for one tick).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct StewardTotals {
    pub domains: u64,
    pub unit_visits: u64,
    pub building_visits: u64,
    pub orders: u64,
    pub resource_searches: u64,
    pub resource_nodes: u64,
    pub site_checks: u64,
}

/// The General's work, summed over ticks (`GeneralWork` is for one tick).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct GeneralTotals {
    pub generals: u64,
    pub unit_visits: u64,
    pub building_visits: u64,
    pub orders: u64,
}

impl From<StewardWork> for StewardTotals {
    fn from(work: StewardWork) -> Self {
        Self {
            domains: work.domains as u64,
            unit_visits: work.unit_visits as u64,
            building_visits: work.building_visits as u64,
            orders: work.orders as u64,
            resource_searches: work.resource_searches as u64,
            resource_nodes: work.resource_nodes as u64,
            site_checks: work.site_checks as u64,
        }
    }
}

impl From<GeneralWork> for GeneralTotals {
    fn from(work: GeneralWork) -> Self {
        Self {
            generals: work.generals as u64,
            unit_visits: work.unit_visits as u64,
            building_visits: work.building_visits as u64,
            orders: work.orders as u64,
        }
    }
}

impl AddAssign<&StewardTotals> for StewardTotals {
    fn add_assign(&mut self, other: &Self) {
        self.domains += other.domains;
        self.unit_visits += other.unit_visits;
        self.building_visits += other.building_visits;
        self.orders += other.orders;
        self.resource_searches += other.resource_searches;
        self.resource_nodes += other.resource_nodes;
        self.site_checks += other.site_checks;
    }
}

impl AddAssign<&GeneralTotals> for GeneralTotals {
    fn add_assign(&mut self, other: &Self) {
        self.generals += other.generals;
        self.unit_visits += other.unit_visits;
        self.building_visits += other.building_visits;
        self.orders += other.orders;
    }
}

/// One played tick. The engine overwrites most of these every tick, so each
/// is read right after the tick and summed by whoever reads a window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct TickRow {
    /// Nanoseconds inside `tick()`: wall time, so noisy.
    pub nanos: u64,
    pub hits: u64,
    /// Units that attacked, summed: attacker-ticks.
    pub attackers: u64,
    pub attack_searches: u64,
    /// First blows struck after a target came into reach, and the ticks they
    /// had waited since contact.
    pub first_blows: u64,
    pub first_blow_wait_ticks: u64,
    /// Food, wood, gold and stone credited this tick, every seat together.
    pub delivered: [u64; 4],
    /// Units whose id showed for the first time this tick.
    pub units_born: u64,
    /// Movement orders seen starting, reaching their goal and given up.
    pub orders_given: u64,
    pub orders_arrived: u64,
    pub orders_abandoned: u64,
    /// Ticks where the delivery bookkeeping contradicted itself (a negative
    /// amount): 0 when the reading of `recent_deliveries` is sound.
    pub income_faults: u64,
    pub steward: StewardTotals,
    pub general: GeneralTotals,
}

/// A delay observed at a tick: how many ticks an order took to show a
/// result. Not additive, so a window keeps the values and takes percentiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Delay {
    /// The tick where it was observed.
    pub tick: u64,
    pub ticks: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Events {
    /// Ticks between an order showing and the unit stepping.
    pub first_steps: Vec<Delay>,
    /// Ticks between an order showing and the unit within two tiles of its goal.
    pub arrivals: Vec<Delay>,
}
