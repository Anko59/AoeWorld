//! Aggregation of a run's records into the JSON report and its summary block.
use crate::perf_scale::stats::{Series, round};
use serde_json::{Value, json};

/// The server's tick budget at 20 Hz.
pub(super) const BUDGET_MS: f64 = 50.0;
const TICK_HZ: f64 = 20.0;

/// One server tick as `GET /match/stats` reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TickRow {
    pub tick: u64,
    pub total_us: u64,
    pub sim_us: u64,
    pub late_us: u64,
    pub orders: u64,
    pub units: u64,
    pub missed: bool,
    pub phases_us: [u64; 3],
}

impl TickRow {
    pub(super) fn from_json(value: &Value) -> Option<Self> {
        let field = |index: usize| value.get(index)?.as_u64();
        Some(Self {
            tick: field(0)?,
            total_us: field(1)?,
            sim_us: field(2)?,
            late_us: field(3)?,
            orders: field(4)?,
            units: field(5)?,
            missed: field(6)? != 0,
            phases_us: [
                value.get(8)?.get("orders")?.as_u64()?,
                value.get(8)?.get("combat")?.as_u64()?,
                value.get(8)?.get("hierarchy")?.as_u64()?,
            ],
        })
    }

    fn total_ms(&self) -> f64 {
        self.total_us as f64 / 1000.0
    }

    fn sim_ms(&self) -> f64 {
        self.sim_us as f64 / 1000.0
    }

    /// Order application, per-session views and queueing: everything but `advance`.
    fn fan_out_ms(&self) -> f64 {
        self.total_us.saturating_sub(self.sim_us) as f64 / 1000.0
    }
}

/// One periodic sample of the machine side of the run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Sample {
    pub seconds: f64,
    pub tick: u64,
    pub units: u64,
    pub server_cpu_cores: f64,
    pub server_rss_mib: u64,
    pub bots_cpu_cores: f64,
    pub bots_rss_mib: f64,
}

fn series(rows: &[TickRow], pick: impl Fn(&TickRow) -> f64) -> Value {
    Series(rows.iter().map(pick).collect()).summary()
}

/// Tick timings of a window of rows.
pub(super) fn window(rows: &[TickRow], wall_seconds: f64) -> Value {
    let missed = rows.iter().filter(|row| row.missed).count();
    let over_budget = rows.iter().filter(|row| row.total_ms() > BUDGET_MS).count();
    let orders: u64 = rows.iter().map(|row| row.orders).sum();
    let ticks_per_second = if wall_seconds > 0.0 {
        rows.len() as f64 / wall_seconds
    } else {
        0.0
    };
    json!({
        "ticks": rows.len(),
        "tick_ms": series(rows, TickRow::total_ms),
        "advance_ms": series(rows, TickRow::sim_ms),
        "fan_out_and_orders_ms": series(rows, TickRow::fan_out_ms),
        "phases_ms": {
            "orders": series(rows, |row| row.phases_us[0] as f64 / 1000.0),
            "combat": series(rows, |row| row.phases_us[1] as f64 / 1000.0),
            "hierarchy": series(rows, |row| row.phases_us[2] as f64 / 1000.0),
        },
        "late_start_ms": series(rows, |row| row.late_us as f64 / 1000.0),
        "missed_ticks": missed,
        "missed_tick_rate": round(missed as f64 / rows.len().max(1) as f64),
        "over_50ms": over_budget,
        "ticks_per_second": round(ticks_per_second),
        "realtime_factor": round(ticks_per_second / TICK_HZ),
        "orders_applied": orders,
        "orders_per_second": round(if wall_seconds > 0.0 { orders as f64 / wall_seconds } else { 0.0 }),
        "units_first": rows.first().map_or(0, |row| row.units),
        "units_last": rows.last().map_or(0, |row| row.units),
        "units_max": rows.iter().map(|row| row.units).max().unwrap_or(0),
    })
}

/// One window per wall-clock minute, by the order the rows arrived in
/// (a row's wall time is its index over the measured ticks per second).
pub(super) fn per_minute(rows: &[TickRow], wall_seconds: f64) -> Vec<Value> {
    if rows.is_empty() || wall_seconds <= 0.0 {
        return Vec::new();
    }
    let minutes = (wall_seconds / 60.0).ceil().max(1.0) as usize;
    let per = rows.len().div_ceil(minutes);
    rows.chunks(per.max(1))
        .enumerate()
        .map(|(index, chunk)| {
            let mut value = window(chunk, wall_seconds / minutes as f64);
            value["minute"] = json!(index + 1);
            value
        })
        .collect()
}

pub(super) fn timeline(samples: &[Sample]) -> Vec<Value> {
    samples
        .iter()
        .map(|s| {
            json!({
                "t_s": round(s.seconds),
                "tick": s.tick,
                "units": s.units,
                "server_cpu_cores": round(s.server_cpu_cores),
                "server_rss_mib": s.server_rss_mib,
                "bots_cpu_cores": round(s.bots_cpu_cores),
                "bots_rss_mib": round(s.bots_rss_mib),
            })
        })
        .collect()
}

/// Totals over the bots' own final reports (`aoe_ai host --report`).
pub(super) fn bot_totals(reports: &[Value]) -> Value {
    let mut total = json!({
        "bots": 0, "decisions": 0, "errors": 0, "closed": 0, "villagers": 0, "army": 0,
        "buildings": 0, "population": 0, "orders_accepted": 0, "orders_rejected": 0,
        "drops": 0, "rejoins": 0, "sessions_dropped": 0, "reconnect_seconds": 0.0,
        "feudal": 0, "build_orders": 0, "buildings_completed": 0, "build_rejections": {},
    });
    let add = |total: &mut Value, key: &str, amount: u64| {
        total[key] = json!(total[key].as_u64().unwrap_or(0) + amount);
    };
    for report in reports
        .iter()
        .flat_map(|r| r.as_array().into_iter().flatten())
    {
        let number = |key: &str| report.get(key).and_then(Value::as_u64).unwrap_or(0);
        add(&mut total, "bots", 1);
        add(&mut total, "decisions", number("decisions"));
        add(&mut total, "errors", number("errors"));
        add(
            &mut total,
            "closed",
            u64::from(report["closed"].as_bool() == Some(true)),
        );
        add(&mut total, "villagers", number("villagers"));
        add(&mut total, "army", number("army"));
        add(&mut total, "buildings", number("buildings"));
        add(&mut total, "population", number("population"));
        add(&mut total, "orders_accepted", number("accepted"));
        add(&mut total, "orders_rejected", number("rejected"));
        add(&mut total, "drops", number("drops"));
        add(
            &mut total,
            "feudal",
            u64::from(report["age"].as_str() == Some("FEUDAL")),
        );
        let stats = report.get("stats").and_then(Value::as_object);
        for (key, value) in stats.into_iter().flatten() {
            let amount = value.as_u64().unwrap_or(0);
            if key == "build_orders" || key == "buildings_completed" {
                add(&mut total, key, amount);
            } else if let Some(reason) = key.strip_prefix("rejected:build:") {
                let by = &mut total["build_rejections"][reason];
                *by = json!(by.as_u64().unwrap_or(0) + amount);
            }
        }
        add(&mut total, "rejoins", number("rejoins"));
        add(
            &mut total,
            "sessions_dropped",
            u64::from(number("drops") > 0),
        );
        let away = report
            .get("reconnect_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        total["reconnect_seconds"] = json!(round(
            total["reconnect_seconds"].as_f64().unwrap_or(0.0) + away
        ));
    }
    total
}

fn ms(value: &Value, key: &str) -> String {
    format!("{:.1}", value[key].as_f64().unwrap_or(0.0))
}

/// The generated block of `docs/scale/match.md` for one report.
pub(super) fn markdown(report: &Value) -> String {
    let (config, setup, run) = (&report["config"], &report["setup"], &report["run"]);
    let (tick, advance, fan) = (
        &run["tick_ms"],
        &run["advance_ms"],
        &run["fan_out_and_orders_ms"],
    );
    let mut text = format!(
        "## Latest run: {label}\n\n\
         Revision `{revision}`{dirty}, {players} players ({humans} human), {tiles} x {tiles} tiles \
         ({map}), population cap {cap}, {minutes:.1} min of play, {hosts} Python host process(es).\n\n\
         | | |\n|---|---|\n\
         | tick total p50 / p95 / p99 / max (ms) | {t50} / {t95} / {t99} / {tmax} |\n\
         | `advance` (simulation) p50 / p95 (ms) | {a50} / {a95} |\n\
         | orders, views and fan-out p50 / p95 (ms) | {f50} / {f95} |\n\
         | missed ticks (late start or over 50 ms) | {missed} of {ticks} ({rate:.1}%) |\n\
         | game speed (1.0 = real time) | {factor} |\n\
         | units at start / end / max | {u0} / {u1} / {umax} |\n\
         | of which seeded by `start_units` | {seeded} |\n\
         | villages with a partial starting kit | {partial} |\n\
         | orders applied per second (mean) | {ops} |\n\
         | server CPU mean / max (cores), RSS max (MiB) | {scpu} / {smax}, {srss} |\n\
         | bots CPU mean / max (cores), memory max (MiB) | {bcpu} / {bmax}, {brss} |\n\
         | match creation / bots connected (s) | {create} / {connect} |\n",
        label = report["label"].as_str().unwrap_or(""),
        revision = report["revision"].as_str().unwrap_or("?"),
        dirty = if report["dirty"] == json!(true) {
            " (dirty tree)"
        } else {
            ""
        },
        players = config["players"],
        humans = config["humans"],
        tiles = config["tiles"],
        map = setup["map_kind"].as_str().unwrap_or("?"),
        cap = config["population_cap"],
        minutes = run["wall_seconds"].as_f64().unwrap_or(0.0) / 60.0,
        hosts = config["hosts"],
        t50 = ms(tick, "p50_ms"),
        t95 = ms(tick, "p95_ms"),
        t99 = ms(tick, "p99_ms"),
        tmax = ms(tick, "max_ms"),
        a50 = ms(advance, "p50_ms"),
        a95 = ms(advance, "p95_ms"),
        f50 = ms(fan, "p50_ms"),
        f95 = ms(fan, "p95_ms"),
        missed = run["missed_ticks"],
        ticks = run["ticks"],
        rate = run["missed_tick_rate"].as_f64().unwrap_or(0.0) * 100.0,
        factor = run["realtime_factor"],
        u0 = run["units_first"],
        u1 = run["units_last"],
        umax = run["units_max"],
        seeded = setup["seeded_units"],
        partial = setup["partial_starting_kits"],
        ops = run["orders_per_second"],
        scpu = report["machine"]["server_cpu_mean_cores"],
        smax = report["machine"]["server_cpu_max_cores"],
        srss = report["machine"]["server_rss_max_mib"],
        bcpu = report["machine"]["bots_cpu_mean_cores"],
        bmax = report["machine"]["bots_cpu_max_cores"],
        brss = report["machine"]["bots_rss_max_mib"],
        create = setup["match_create_seconds"],
        connect = setup["bots_connect_seconds"],
    );
    let bots = &report["bots"];
    text.push_str(&format!(
        "\nBots at the end: {} reporting, {} villagers, {} soldiers, {} buildings, {} errors, \
         {} accepted and {} rejected orders. Sessions: {} dropped and rejoined ({} drops, {} rejoins, \
         {} s away in all), {} permanently lost. {} in Feudal Age; {} build orders sent, {} \
         buildings completed, build refusals {}.\n",
        bots["bots"],
        bots["villagers"],
        bots["army"],
        bots["buildings"],
        bots["errors"],
        bots["orders_accepted"],
        bots["orders_rejected"],
        bots["sessions_dropped"],
        bots["drops"],
        bots["rejoins"],
        bots["reconnect_seconds"],
        bots["closed"],
        bots["feudal"],
        bots["build_orders"],
        bots["buildings_completed"],
        bots["build_rejections"],
    ));
    if let Some(browser) = report.get("browser").filter(|b| !b.is_null()) {
        text.push_str(&format!("\nBrowser: {}\n", browser_line(browser)));
    }
    text.push_str("\nPer minute (tick p50 / p95 ms, missed, units at end):\n\n| min | p50 | p95 | missed | units |\n|---|---|---|---|---|\n");
    for window in report["per_minute"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            window["minute"],
            ms(&window["tick_ms"], "p50_ms"),
            ms(&window["tick_ms"], "p95_ms"),
            window["missed_ticks"],
            window["units_last"],
        ));
    }
    text
}

pub(super) fn browser_line(browser: &Value) -> String {
    let phases = browser["phases"].as_array().cloned().unwrap_or_default();
    let error = browser.get("error").and_then(Value::as_str);
    if phases.is_empty() {
        return format!("not measured: {}", error.unwrap_or("no phase finished"));
    }
    let measured = phases
        .iter()
        .map(|phase| {
            format!(
                "{} {:.1} fps mean, frame p50 {:.1} / p95 {:.1} ms ({} frames)",
                phase["name"].as_str().unwrap_or("?"),
                phase["fps_mean"].as_f64().unwrap_or(0.0),
                phase["frame_ms_p50"].as_f64().unwrap_or(0.0),
                phase["frame_ms_p95"].as_f64().unwrap_or(0.0),
                phase["frames"],
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    error.map_or(measured.clone(), |error| {
        format!("{measured} (then failed: {error})")
    })
}
