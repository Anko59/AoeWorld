//! The terminal table, and the section of the pull request comment: one block
//! per scenario, one line per number, one column per window, in plain words.
use super::{Played, staged};
use crate::game_score::judge::Comparison;
use crate::game_score::measure::Measurement;
use crate::game_score::windows::{Minute, WindowReport};

type Row = (&'static str, fn(&WindowReport) -> String);

/// 12345 as "12,345".
pub(super) fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut text = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

fn pair(a: u64, b: u64) -> String {
    format!("{} / {}", grouped(a), grouped(b))
}

fn quantiles(p50: Option<u64>, p95: Option<u64>) -> String {
    match (p50, p95) {
        (Some(a), Some(b)) => pair(a, b),
        _ => "none".to_owned(),
    }
}

fn percent(permille: u64) -> String {
    format!("{}%", (permille + 5) / 10)
}

/// Orders by one issuer.
fn issuer(window: &WindowReport, name: &str) -> u64 {
    window
        .service
        .management
        .orders_by_issuer
        .get(name)
        .map_or(0, |totals| totals.orders)
}

/// What the table shows, in order. Every row is one number of a window.
const ROWS: [Row; 22] = [
    ("units alive at the end", |w| grouped(w.service.size.units)),
    ("units / buildings produced", |w| {
        pair(
            w.service.size.units_produced,
            w.service.size.buildings_produced,
        )
    }),
    ("villagers standing idle (share)", |w| {
        percent(w.service.size.idle_villagers_permille)
    }),
    ("movement orders given", |w| {
        grouped(w.service.arrival.orders_given)
    }),
    ("orders arrived (share of orders ended)", |w| {
        let arrival = &w.service.arrival;
        let share = arrival.arrived_permille.map_or("-".to_owned(), percent);
        format!("{} ({share})", grouped(arrival.orders_arrived))
    }),
    ("order to arrival, p50 / p95 (ticks)", |w| {
        let delay = &w.service.arrival.order_to_arrival_ticks;
        quantiles(delay.p50, delay.p95)
    }),
    ("order to first step, p50 / p95 (ticks)", |w| {
        let delay = &w.service.arrival.order_to_first_step_ticks;
        quantiles(delay.p50, delay.p95)
    }),
    ("route searches started / answers from the cache", |w| {
        pair(
            w.service.planning.route_searches,
            w.service.planning.cache_hits,
        )
    }),
    ("route orders stuck / expired", |w| {
        pair(w.service.planning.stuck, w.service.planning.expired)
    }),
    ("movers waiting for a route, end / peak", |w| {
        pair(
            w.service.planning.waiting_end,
            w.service.planning.waiting_peak,
        )
    }),
    ("wait for a route, p95 end / worst (ticks)", |w| {
        pair(
            w.service.planning.wait_p95_ticks_end,
            w.service.planning.wait_p95_ticks_peak,
        )
    }),
    ("Steward orders / unit visits", |w| {
        let steward = &w.service.management.steward;
        pair(steward.orders, steward.unit_visits)
    }),
    ("General orders / unit visits", |w| {
        let general = &w.service.management.general;
        pair(general.orders, general.unit_visits)
    }),
    ("Vassal / Rust AI orders", |w| {
        pair(issuer(w, "vassal"), issuer(w, "rust_ai"))
    }),
    ("hits landed", |w| grouped(w.service.fights.hits)),
    ("first blow after contact (ticks, mean)", |w| {
        let fights = &w.service.fights;
        match fights.first_blows {
            0 => "none".to_owned(),
            blows => format!("{:.1}", fights.first_blow_wait_ticks as f64 / blows as f64),
        }
    }),
    ("kills / losses", |w| {
        pair(w.service.fights.kills, w.service.fights.losses)
    }),
    ("resources delivered", |w| {
        grouped(w.service.economy.delivered_total)
    }),
    ("seconds inside the ticks", |w| {
        format!("{:.1}", w.cost.tick_seconds)
    }),
    ("tick ms p50 / p95 / p99 / max", |w| {
        let ms = &w.cost.tick_ms;
        format!("{:.0}/{:.0}/{:.0}/{:.0}", ms.p50, ms.p95, ms.p99, ms.max)
    }),
    ("peak memory (MiB)", |w| grouped(w.cost.peak_memory_mib)),
    ("cost per arrival / 1,000 resources / hit ({unit})", |w| {
        let cost = &w.efficiency;
        let each = |value: Option<f64>| value.map_or("-".to_owned(), |v| format!("{v:.1}"));
        format!(
            "{}/{}/{}",
            each(cost.per_arrival),
            each(cost.per_thousand_resources_delivered),
            each(cost.per_hit)
        )
    }),
];

fn workers(count: usize) -> String {
    match count {
        1 => "1 worker".to_owned(),
        n => format!("{n} workers"),
    }
}

/// One line in words about one comparison.
pub(super) fn describe(comparison: &Comparison, hash_every: u64) -> String {
    let pair = format!(
        "{} and {}",
        workers(comparison.base_workers),
        workers(comparison.workers)
    );
    if comparison.holds() {
        let orders = match comparison.same_orders {
            Some(_) => format!("the same hash every {hash_every} ticks and the same orders"),
            None => format!("the same hash every {hash_every} ticks (no order log kept)"),
        };
        let numbers = if comparison.same_numbers {
            ", the same numbers"
        } else {
            ", DIFFERENT numbers"
        };
        return format!("{pair}: {orders}{numbers}");
    }
    let tick = comparison
        .first_differing_tick
        .map_or_else(|| "unknown".to_owned(), |tick| tick.to_string());
    let mut text = format!("{pair}: DIFFERENT GAMES, first differing tick {tick}");
    if let Some(difference) = &comparison.hashes {
        text.push_str(&format!("; world: {}", difference.what));
    }
    if let Some(difference) = &comparison.orders {
        text.push_str(&format!(
            "; first differing order is number {}",
            difference.index
        ));
    }
    text
}

fn run_line(run: &Measurement) -> String {
    format!(
        "{} {:.1} s (start {:.1})",
        workers(run.workers),
        run.native_seconds,
        run.setup_seconds
    )
}

/// Numbers one per game minute after their label, wrapped when a line would
/// pass 100 characters.
fn per_minute(label: &str, minutes: &[Minute], value: fn(&Minute) -> u64) -> String {
    let mut text = label.to_owned();
    let mut width = label.len();
    for minute in minutes {
        let number = format!(" {}", grouped(value(minute)));
        if width + number.len() > 100 {
            text.push_str("\n ");
            width = 1;
        }
        width += number.len();
        text.push_str(&number);
    }
    text.push('\n');
    text
}

/// The block of one scenario.
fn block(played: &Played) -> String {
    let scenario = played.scenario;
    let Some(base) = played.runs.first() else {
        return format!("{}: not played\n", scenario.id);
    };
    let windows = &base.windows;
    let unit = match windows
        .first()
        .map(|window| window.efficiency.basis.as_str())
    {
        Some("instructions") => "instructions",
        _ => "wall ms",
    };
    let at_a_choke = windows.iter().any(|w| w.service.choke.is_some());
    let mut rows: Vec<Row> = ROWS
        .iter()
        .copied()
        .filter(|(name, _)| !(at_a_choke && staged::is_empty_in_a_staged_scenario(name)))
        .collect();
    rows.extend(staged::rows(windows));
    let names: Vec<String> = rows
        .iter()
        .map(|(name, _)| name.replace("{unit}", unit))
        .collect();
    let label = names.iter().map(String::len).max().unwrap_or(0);
    let mut text = format!(
        "{}: {}\n  {} seats, {} ticks ({:.1} game minutes), measured at {}\n",
        scenario.id,
        scenario.what,
        base.seats,
        grouped(base.ticks),
        base.ticks as f64 / 600.0,
        workers(base.workers)
    );
    text.push_str(&format!("{:label$}", ""));
    for window in windows {
        text.push_str(&format!("  {:>16}", window.id));
    }
    text.push_str(&format!("\n{:label$}", ""));
    for window in windows {
        let ticks = format!("{}-{}", window.from_tick, window.to_tick);
        text.push_str(&format!("  {ticks:>16}"));
    }
    text.push('\n');
    for (name, (_, value)) in names.iter().zip(rows) {
        text.push_str(&format!("{name:label$}"));
        for window in windows {
            text.push_str(&format!("  {:>16}", value(window)));
        }
        text.push('\n');
    }
    if base.series.len() > 1 {
        if at_a_choke {
            text.push_str(&per_minute(
                "units across the line per game minute:",
                &base.series,
                |minute| minute.service.choke.map_or(0, |choke| choke.crossed),
            ));
        } else {
            text.push_str(&per_minute(
                "resources delivered per game minute:",
                &base.series,
                |minute| minute.service.economy.delivered_total,
            ));
        }
    }
    if !scenario.note.is_empty() {
        text.push_str(&format!("note: {}\n", scenario.note));
    }
    for comparison in &played.verdict.against {
        text.push_str(&format!(
            "determinism, {}\n",
            describe(comparison, played.plan.hash_every)
        ));
    }
    if let Some(diagnosis) = &played.verdict.diagnosis {
        text.push_str(&format!(
            "replay with a hash at every tick: {}\n",
            describe(diagnosis, 1)
        ));
    }
    if played.verdict.against.is_empty() {
        text.push_str("determinism: not checked (one run only)\n");
    }
    let runs: Vec<String> = played.runs.iter().map(run_line).collect();
    text.push_str(&format!("native seconds: {}\n", runs.join(", ")));
    text
}

/// The whole table: a title, then one block per scenario.
pub(in crate::game_score) fn table(title: &str, played: &[Played]) -> String {
    let blocks: Vec<String> = played.iter().map(block).collect();
    format!("{title}\n\n{}", blocks.join("\n"))
}
