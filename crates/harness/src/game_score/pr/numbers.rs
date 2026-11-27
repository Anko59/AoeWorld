//! The numbers the pull request section names: how to call them, which way is better, and
//! under what name a pull request declares a change to one. Paths are those of a window of the
//! report (`service.*` without its prefix, `efficiency.*`, `cost.*`); a number the table does
//! not know is still compared and shown, under its path.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Better {
    Higher,
    Lower,
    Neither,
}

pub(super) struct Known {
    pub path: &'static str,
    pub label: &'static str,
    pub better: Better,
    /// The name in `game-score: <scenario> <name> <change> because <reason>`; `None`: the number
    /// is shown but not judged.
    pub declare: Option<&'static str>,
    /// Shown even when no merge base is there to compare with.
    pub headline: bool,
}

const fn known(
    path: &'static str,
    label: &'static str,
    better: Better,
    declare: Option<&'static str>,
    headline: bool,
) -> Known {
    Known {
        path,
        label,
        better,
        declare,
        headline,
    }
}

use Better::{Higher, Lower, Neither};

pub(super) const KNOWN: &[Known] = &[
    known("size.units", "units alive at the end", Neither, None, true),
    known(
        "size.buildings",
        "buildings standing at the end",
        Neither,
        None,
        false,
    ),
    known(
        "size.villagers",
        "villagers at the end",
        Neither,
        None,
        false,
    ),
    known(
        "size.units_produced",
        "units produced",
        Neither,
        None,
        false,
    ),
    known(
        "size.buildings_produced",
        "buildings produced",
        Neither,
        None,
        false,
    ),
    known(
        "size.idle_villagers_permille",
        "villagers standing idle",
        Lower,
        Some("idle"),
        true,
    ),
    known(
        "arrival.orders_given",
        "movement orders given",
        Neither,
        None,
        true,
    ),
    known(
        "arrival.orders_arrived",
        "orders arrived",
        Neither,
        None,
        false,
    ),
    known(
        "arrival.orders_abandoned",
        "orders given up",
        Neither,
        None,
        false,
    ),
    known(
        "arrival.arrived_permille",
        "share of orders arrived",
        Higher,
        Some("arrived"),
        true,
    ),
    known(
        "arrival.order_to_arrival_ticks.p50",
        "order to arrival, p50 (ticks)",
        Neither,
        None,
        false,
    ),
    known(
        "arrival.order_to_arrival_ticks.p95",
        "order to arrival, p95 (ticks)",
        Lower,
        Some("arrival-p95"),
        true,
    ),
    known(
        "arrival.order_to_first_step_ticks.p50",
        "order to first step, p50 (ticks)",
        Neither,
        None,
        false,
    ),
    known(
        "arrival.order_to_first_step_ticks.p95",
        "order to first step, p95 (ticks)",
        Lower,
        Some("first-step-p95"),
        true,
    ),
    known(
        "planning.route_searches",
        "route searches started",
        Neither,
        None,
        true,
    ),
    known(
        "planning.cache_hits",
        "route answers from the cache",
        Neither,
        None,
        false,
    ),
    known(
        "planning.stuck",
        "route orders stuck",
        Lower,
        Some("stuck"),
        true,
    ),
    known(
        "planning.expired",
        "route orders expired",
        Lower,
        Some("expired"),
        false,
    ),
    known(
        "planning.waiting_peak",
        "movers waiting for a route, peak",
        Lower,
        Some("waiting"),
        true,
    ),
    known(
        "planning.wait_p95_ticks_peak",
        "wait for a route, p95 at the worst sample (ticks)",
        Lower,
        Some("wait-p95"),
        false,
    ),
    known(
        "planning.route_orders",
        "route orders",
        Neither,
        None,
        false,
    ),
    known(
        "planning.solved_when_given",
        "route orders answered when given",
        Neither,
        None,
        false,
    ),
    known(
        "planning.waiting_end",
        "movers waiting for a route, at the end",
        Neither,
        None,
        false,
    ),
    known(
        "management.steward.orders",
        "Steward orders",
        Neither,
        None,
        true,
    ),
    known(
        "management.steward.unit_visits",
        "Steward unit visits",
        Neither,
        None,
        false,
    ),
    known(
        "management.general.orders",
        "General orders",
        Neither,
        None,
        true,
    ),
    known(
        "management.general.unit_visits",
        "General unit visits",
        Neither,
        None,
        false,
    ),
    known(
        "management.orders_by_issuer.rust_ai.orders",
        "Rust AI orders",
        Neither,
        None,
        true,
    ),
    known(
        "management.orders_by_issuer.vassal.orders",
        "Vassal orders",
        Neither,
        None,
        false,
    ),
    known(
        "management.orders_by_issuer.automation.orders",
        "automation orders",
        Neither,
        None,
        false,
    ),
    known("fights.hits", "hits landed", Neither, None, true),
    known(
        "fights.first_blows",
        "first blows after contact",
        Neither,
        None,
        false,
    ),
    known(
        "fights.first_blow_wait_ticks",
        "ticks the first blows waited",
        Neither,
        None,
        false,
    ),
    known("fights.kills", "kills", Neither, None, false),
    known(
        "fights.losses",
        "units and buildings lost",
        Neither,
        None,
        false,
    ),
    known(
        "economy.delivered.food",
        "food delivered",
        Neither,
        None,
        false,
    ),
    known(
        "economy.delivered.wood",
        "wood delivered",
        Neither,
        None,
        false,
    ),
    known(
        "economy.delivered.gold",
        "gold delivered",
        Neither,
        None,
        false,
    ),
    known(
        "economy.delivered.stone",
        "stone delivered",
        Neither,
        None,
        false,
    ),
    known(
        "economy.delivered_total",
        "resources delivered",
        Higher,
        Some("delivered"),
        true,
    ),
    known(
        "choke.units_sent",
        "units sent to the choke",
        Neither,
        None,
        true,
    ),
    known(
        "choke.crossed",
        "units across the line",
        Higher,
        Some("crossed"),
        true,
    ),
    known(
        "choke.crossed_per_minute_peak",
        "most across the line in a game minute",
        Higher,
        Some("crossing-rate"),
        true,
    ),
    known(
        "choke.first_crossing_tick",
        "first unit across (tick)",
        Lower,
        Some("first-crossing"),
        true,
    ),
    known(
        "choke.last_crossing_tick",
        "last unit across (tick)",
        Lower,
        Some("last-crossing"),
        true,
    ),
    known(
        "choke.waiting_near_side",
        "units still waiting on the near side",
        Lower,
        Some("waiting-near-side"),
        true,
    ),
    known(
        "choke.standing_still_inside_ticks",
        "longest stand still inside the neck (ticks)",
        Lower,
        Some("still-inside"),
        true,
    ),
    known(
        "choke.arrived_at_goal",
        "units within two tiles of their goal",
        Higher,
        Some("at-goal"),
        true,
    ),
    known(
        "battle.first_blow_tick",
        "first blow (tick)",
        Lower,
        Some("first-blow"),
        true,
    ),
    known("battle.fought", "units that fought", Neither, None, true),
    known(
        "battle.queued",
        "units that only queued behind",
        Lower,
        Some("queued"),
        true,
    ),
    known(
        "battle.fight_ended_tick",
        "fight ended (tick)",
        Neither,
        None,
        true,
    ),
    known(
        "battle.alive_seat_0_permille",
        "seat 0 alive at the end",
        Neither,
        None,
        true,
    ),
    known(
        "battle.alive_seat_1_permille",
        "seat 1 alive at the end",
        Neither,
        None,
        true,
    ),
    known(
        "efficiency.per_arrival",
        "cost per arrival",
        Lower,
        Some("cost-per-arrival"),
        false,
    ),
    known(
        "efficiency.per_thousand_resources_delivered",
        "cost per 1,000 resources delivered",
        Lower,
        Some("cost-per-resources"),
        false,
    ),
    known(
        "efficiency.per_hit",
        "cost per hit",
        Lower,
        Some("cost-per-hit"),
        false,
    ),
    known(
        "efficiency.per_unit_produced",
        "cost per unit produced",
        Lower,
        Some("cost-per-unit"),
        false,
    ),
];

pub(super) fn find(path: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|known| known.path == path)
}

/// The words for `path`: its label, or the path itself for a number the table does not know.
pub(super) fn label(path: &str) -> String {
    find(path).map_or_else(|| path.to_owned(), |known| known.label.to_owned())
}

pub(super) fn better(path: &str) -> Better {
    find(path).map_or(Neither, |known| known.better)
}

/// The number a declaration named `key` stands for.
pub(super) fn by_declaration(key: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|known| known.declare == Some(key))
}

/// The smallest absolute move of a judged number that can be a finding, whatever its relative
/// size: a count of 2 that becomes 7 is not one. Shares are kept in parts per thousand (10 is one
/// percentage point), ticks are the game's clock (3 is a third of a second).
pub(super) fn floor(path: &str) -> f64 {
    if path.contains("ticks") { 3.0 } else { 10.0 }
}

/// The names a declaration may use, for the line that says which one a typo should have been.
pub(super) fn declarable() -> Vec<&'static str> {
    KNOWN.iter().filter_map(|known| known.declare).collect()
}

/// A value in words: shares kept in parts per thousand read as percent, whole numbers get
/// thousands separators, efficiency keeps its decimals.
pub(super) fn show(path: &str, value: Option<f64>) -> String {
    let Some(value) = value else {
        return "none".to_owned();
    };
    if path.ends_with("_permille") {
        return format!("{:.1}%", value / 10.0);
    }
    // A whole number, or one of thousands whose decimals say nothing (a median of two counts).
    if (value.fract() == 0.0 || value.abs() >= 1000.0) && value.abs() < 9e15 {
        return grouped(value.round() as i64);
    }
    format!("{value:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

/// 12345 as "12,345", -3 as "-3".
pub(super) fn grouped(number: i64) -> String {
    let digits = number.unsigned_abs().to_string();
    let mut text = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    if number < 0 {
        text.insert(0, '-');
    }
    text
}

#[cfg(test)]
#[path = "tests/numbers.rs"]
mod tests;
