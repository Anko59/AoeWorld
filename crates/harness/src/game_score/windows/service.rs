//! What the game did in a window of ticks, in whole numbers: the same match
//! gives the same ones at any worker count, so two runs can be compared
//! exactly. (Wall time and memory are in `cost`.)
use crate::game_score::counters::{
    Delay, Events, GeneralTotals, ISSUERS, IssuerTotals, RouteTotals, Sample, StewardTotals,
    TickRow,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Everything a run recorded, to read windows from.
#[derive(Clone, Copy)]
pub(in crate::game_score) struct Record<'a> {
    /// `rows[n - 1]` is tick `n`.
    pub rows: &'a [TickRow],
    pub samples: &'a [Sample],
    pub events: &'a Events,
}

/// A count of observations and the nearest-rank 50th and 95th percentiles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Quantiles {
    pub samples: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
}

impl Quantiles {
    pub fn of(mut values: Vec<u64>) -> Self {
        values.sort_unstable();
        let rank = |percent: usize| {
            (!values.is_empty()).then(|| values[(values.len() * percent).div_ceil(100) - 1])
        };
        Self {
            samples: values.len() as u64,
            p50: rank(50),
            p95: rank(95),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Size {
    /// At the end of the window.
    pub units: u64,
    pub buildings: u64,
    pub villagers: u64,
    pub idle_villagers: u64,
    /// Share of villagers standing idle, in parts per thousand, over the
    /// samples of the window.
    pub idle_villagers_permille: u64,
    /// Units and buildings whose id showed for the first time in the window.
    pub units_produced: u64,
    pub buildings_produced: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Arrival {
    /// Movement orders seen starting in the window.
    pub orders_given: u64,
    /// Orders whose unit got within two tiles of the goal in the window.
    pub orders_arrived: u64,
    /// Orders replaced, ended far from the goal or lost with their unit.
    pub orders_abandoned: u64,
    /// Arrived orders in parts per thousand of the orders that ended.
    pub arrived_permille: Option<u64>,
    /// From the order showing to the arrival.
    pub order_to_arrival_ticks: Quantiles,
    /// From the order showing to the unit stepping (its position changing).
    pub order_to_first_step_ticks: Quantiles,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Planning {
    pub route_orders: u64,
    pub route_searches: u64,
    pub solved_when_given: u64,
    pub cache_hits: u64,
    pub stuck: u64,
    pub expired: u64,
    pub approached: u64,
    /// Movers waiting for a route, at the end and the most at a sample.
    pub waiting_end: u64,
    pub waiting_peak: u64,
    /// The 95th percentile of their wait so far (ticks), at the end and the
    /// worst at a sample; and the longest single wait at a sample.
    pub wait_p95_ticks_end: u64,
    pub wait_p95_ticks_peak: u64,
    pub oldest_wait_ticks_peak: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Management {
    /// The Steward's and the General's work, summed over the ticks.
    pub steward: StewardTotals,
    pub general: GeneralTotals,
    /// Orders by who issued them (the Vassal keeps no per-tick work counter:
    /// its orders are its work).
    pub orders_by_issuer: BTreeMap<String, IssuerTotals>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Fights {
    pub hits: u64,
    pub attacker_ticks: u64,
    pub attack_searches: u64,
    /// First blows after contact, and the ticks they waited in all.
    pub first_blows: u64,
    pub first_blow_wait_ticks: u64,
    pub kills: u64,
    /// Units and buildings lost.
    pub losses: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Delivered {
    pub food: u64,
    pub wood: u64,
    pub gold: u64,
    pub stone: u64,
}

impl Delivered {
    pub fn total(&self) -> u64 {
        self.food + self.wood + self.gold + self.stone
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Economy {
    pub delivered: Delivered,
    pub delivered_total: u64,
    /// Ticks where the reading of the engine's delivery window contradicted
    /// itself: always 0 unless the bookkeeping of `income` is wrong.
    pub income_faults: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Service {
    pub size: Size,
    pub arrival: Arrival,
    pub planning: Planning,
    pub management: Management,
    pub fights: Fights,
    pub economy: Economy,
}

fn sample_at(samples: &[Sample], tick: u64) -> Result<&Sample, String> {
    samples
        .iter()
        .find(|sample| sample.tick == tick)
        .ok_or_else(|| format!("no sample was taken at tick {tick}"))
}

/// What grew from `a` to `b` (the counters never shrink).
fn grew(a: u64, b: u64) -> u64 {
    b.saturating_sub(a)
}

fn routes_between(a: &RouteTotals, b: &RouteTotals) -> RouteTotals {
    RouteTotals {
        orders: grew(a.orders, b.orders),
        searches_started: grew(a.searches_started, b.searches_started),
        solved_at_issue: grew(a.solved_at_issue, b.solved_at_issue),
        cache_hits: grew(a.cache_hits, b.cache_hits),
        stuck: grew(a.stuck, b.stuck),
        expired: grew(a.expired, b.expired),
        approached: grew(a.approached, b.approached),
    }
}

fn issuer_between(a: &IssuerTotals, b: &IssuerTotals) -> IssuerTotals {
    IssuerTotals {
        decisions: grew(a.decisions, b.decisions),
        orders: grew(a.orders, b.orders),
        accepted: grew(a.accepted, b.accepted),
        route_searches: grew(a.route_searches, b.route_searches),
    }
}

/// The ticks `from + 1` through `to`, given a sample at each end.
pub(in crate::game_score) fn service(
    record: Record<'_>,
    from: u64,
    to: u64,
) -> Result<Service, String> {
    let rows = record.rows.get(from as usize..to as usize).ok_or_else(|| {
        format!(
            "window {from}-{to} is longer than the {} ticks played",
            record.rows.len()
        )
    })?;
    let (first, last) = (
        sample_at(record.samples, from)?,
        sample_at(record.samples, to)?,
    );
    let inside: Vec<&Sample> = record
        .samples
        .iter()
        .filter(|sample| sample.tick > from && sample.tick <= to)
        .collect();
    let peak = |value: fn(&Sample) -> u64| inside.iter().map(|s| value(s)).max().unwrap_or(0);
    let mut row = TickRow::default();
    let (mut steward, mut general) = (StewardTotals::default(), GeneralTotals::default());
    let mut delivered = [0_u64; 4];
    for tick in rows {
        row.hits += tick.hits;
        row.attackers += tick.attackers;
        row.attack_searches += tick.attack_searches;
        row.first_blows += tick.first_blows;
        row.first_blow_wait_ticks += tick.first_blow_wait_ticks;
        row.units_born += tick.units_born;
        row.orders_given += tick.orders_given;
        row.orders_arrived += tick.orders_arrived;
        row.orders_abandoned += tick.orders_abandoned;
        row.income_faults += tick.income_faults;
        steward += &tick.steward;
        general += &tick.general;
        for (sum, amount) in delivered.iter_mut().zip(tick.delivered) {
            *sum += amount;
        }
    }
    let (villagers, idle) = inside
        .iter()
        .fold((0, 0), |(v, i), s| (v + s.villagers, i + s.idle_villagers));
    let routes = routes_between(&first.routes, &last.routes);
    let delays = |all: &[Delay]| {
        Quantiles::of(
            all.iter()
                .filter(|delay| delay.tick > from && delay.tick <= to)
                .map(|delay| delay.ticks)
                .collect(),
        )
    };
    let ended = row.orders_arrived + row.orders_abandoned;
    let orders_by_issuer = ISSUERS
        .iter()
        .zip(first.issuers.iter().zip(&last.issuers))
        .map(|(name, (a, b))| ((*name).to_owned(), issuer_between(a, b)))
        .collect();
    let delivered = Delivered {
        food: delivered[0],
        wood: delivered[1],
        gold: delivered[2],
        stone: delivered[3],
    };
    Ok(Service {
        size: Size {
            units: last.units,
            buildings: last.buildings,
            villagers: last.villagers,
            idle_villagers: last.idle_villagers,
            idle_villagers_permille: (idle * 1000).checked_div(villagers).unwrap_or(0),
            units_produced: row.units_born,
            buildings_produced: grew(first.buildings_born, last.buildings_born),
        },
        arrival: Arrival {
            orders_given: row.orders_given,
            orders_arrived: row.orders_arrived,
            orders_abandoned: row.orders_abandoned,
            arrived_permille: (row.orders_arrived * 1000).checked_div(ended),
            order_to_arrival_ticks: delays(&record.events.arrivals),
            order_to_first_step_ticks: delays(&record.events.first_steps),
        },
        planning: Planning {
            route_orders: routes.orders,
            route_searches: routes.searches_started,
            solved_when_given: routes.solved_at_issue,
            cache_hits: routes.cache_hits,
            stuck: routes.stuck,
            expired: routes.expired,
            approached: routes.approached,
            waiting_end: last.waiting,
            waiting_peak: peak(|s| s.waiting),
            wait_p95_ticks_end: last.wait_p95_ticks,
            wait_p95_ticks_peak: peak(|s| s.wait_p95_ticks),
            oldest_wait_ticks_peak: peak(|s| s.oldest_wait_ticks),
        },
        management: Management {
            steward,
            general,
            orders_by_issuer,
        },
        fights: Fights {
            hits: row.hits,
            attacker_ticks: row.attackers,
            attack_searches: row.attack_searches,
            first_blows: row.first_blows,
            first_blow_wait_ticks: row.first_blow_wait_ticks,
            kills: grew(first.kills, last.kills),
            losses: grew(first.losses, last.losses),
        },
        economy: Economy {
            delivered,
            delivered_total: delivered.total(),
            income_faults: row.income_faults,
        },
    })
}
