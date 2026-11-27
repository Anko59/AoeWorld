//! Synthetic game score reports (`schema` 1), as the game score writes them: the same keys and
//! the same shape, plausible numbers. Small enough to read, real enough that a number added to
//! the report tomorrow breaks no test that does not mean to say so.
use crate::game_score::pr::spread::{Entry, Measured, Spread, judged};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) const MAP: &str = "ce1c8d7e58352ce6ce98c6c58c05a15b091f2cc151a1f9cc123e338cc2d808e0";

fn issuer(orders: u64) -> Value {
    json!({"accepted": orders / 2, "decisions": orders, "orders": orders, "route_searches": orders / 4})
}

pub(crate) fn service() -> Value {
    json!({
        "arrival": {
            "arrived_permille": 971,
            "order_to_arrival_ticks": {"p50": 0, "p95": 61, "samples": 11840},
            "order_to_first_step_ticks": {"p50": 1, "p95": 1, "samples": 10206},
            "orders_abandoned": 346,
            "orders_arrived": 11840,
            "orders_given": 12314
        },
        "economy": {
            "delivered": {"food": 29061, "gold": 170, "stone": 0, "wood": 14816},
            "delivered_total": 44047,
            "income_faults": 0
        },
        "fights": {
            "attack_searches": 9, "attacker_ticks": 614, "first_blow_wait_ticks": 0,
            "first_blows": 6, "hits": 25, "kills": 1, "losses": 2
        },
        "management": {
            "general": {"building_visits": 0, "generals": 0, "orders": 0, "unit_visits": 0},
            "orders_by_issuer": {
                "auto_scout": issuer(0), "automation": issuer(14518), "general": issuer(0),
                "player": issuer(0), "python_ai": issuer(0), "rust_ai": issuer(3429),
                "steward": issuer(0), "vassal": issuer(0)
            },
            "steward": {
                "building_visits": 0, "domains": 0, "orders": 0, "resource_nodes": 0,
                "resource_searches": 0, "site_checks": 0, "unit_visits": 0
            }
        },
        "planning": {
            "approached": 0, "cache_hits": 5458, "expired": 0, "oldest_wait_ticks_peak": 593,
            "route_orders": 16580, "route_searches": 7230, "solved_when_given": 12257,
            "stuck": 3913, "wait_p95_ticks_end": 0, "wait_p95_ticks_peak": 593,
            "waiting_end": 0, "waiting_peak": 2
        },
        "size": {
            "buildings": 581, "buildings_produced": 236, "idle_villagers": 2,
            "idle_villagers_permille": 1, "units": 1344, "units_produced": 648, "villagers": 1199
        }
    })
}

pub(crate) fn window(id: &str, from: u64, to: u64) -> Value {
    json!({
        "id": id, "from_tick": from, "to_tick": to,
        "service": service(),
        "cost": {
            "instructions": null, "peak_memory_mib": 327,
            "tick_ms": {"max": 56.348, "mean": 3.374, "p50": 2.202, "p95": 8.166, "p99": 23.558},
            "tick_seconds": 6.748
        },
        "efficiency": {
            "basis": "wall_time", "per_arrival": 0.57, "per_hit": 269.92,
            "per_thousand_resources_delivered": 153.2, "per_unit_produced": 10.414,
            "unit": "milliseconds"
        }
    })
}

/// The determinism block of a scenario played at 1 and 15 workers that agreed.
pub(crate) fn agreeing() -> Value {
    json!({
        "holds": true, "fixed_workers": null, "diagnosis": null,
        "runs": [
            {"workers": 1, "native_seconds": 57.2, "orders": 41938},
            {"workers": 15, "native_seconds": 48.2, "orders": 41938}
        ],
        "against": [{
            "base_workers": 1, "workers": 15, "same_hashes": true, "same_orders": true,
            "same_numbers": true, "first_differing_tick": null, "hashes": null, "orders": null
        }]
    })
}

pub(crate) fn scenario(id: &str, ticks: u64, windows: Vec<Value>) -> Value {
    json!({
        "id": id, "what": "a fixture", "note": "", "seats": 100, "seed": 1, "ticks": ticks,
        "start": {"kind": "fresh"}, "hash_every": 10, "sample_every": 50,
        "measured_at_workers": 1,
        "windows": windows,
        "series": [
            {"minute": 1, "service": {"economy": {"delivered_total": 100}}},
            {"minute": 2, "service": {"economy": {"delivered_total": 250}}}
        ],
        "determinism": agreeing()
    })
}

pub(crate) fn natural() -> Value {
    scenario(
        "natural",
        10_000,
        vec![
            window("early", 1_000, 3_000),
            window("middle", 8_000, 10_000),
            window("whole", 0, 10_000),
        ],
    )
}

pub(crate) fn late() -> Value {
    scenario(
        "late",
        1_200,
        vec![window("late", 600, 1_200), window("whole", 0, 1_200)],
    )
}

pub(crate) fn report(scenarios: Vec<Value>) -> Value {
    json!({
        "schema": 1, "name": "game score", "table": "a table",
        "header": {"map_hash": MAP, "revision": "0123456789abcdef", "cores": 15},
        "scenarios": scenarios
    })
}

/// Both scenarios of the pull request set.
pub(crate) fn standard() -> Value {
    report(vec![natural(), late()])
}

fn window_mut<'a>(report: &'a mut Value, scenario: &str, window: &str) -> &'a mut Value {
    report["scenarios"]
        .as_array_mut()
        .expect("scenarios")
        .iter_mut()
        .find(|s| s["id"] == scenario)
        .unwrap_or_else(|| panic!("no scenario {scenario}"))["windows"]
        .as_array_mut()
        .expect("windows")
        .iter_mut()
        .find(|w| w["id"] == window)
        .unwrap_or_else(|| panic!("no window {window}"))
}

/// Sets the number at `path` (`service.arrival.arrived_permille`, `cost.tick_seconds`, ...) of one
/// window; the number must exist.
pub(crate) fn set(report: &mut Value, scenario: &str, window: &str, path: &str, value: Value) {
    let pointer = format!("/{}", path.replace('.', "/"));
    *window_mut(report, scenario, window)
        .pointer_mut(&pointer)
        .unwrap_or_else(|| panic!("no number {path}")) = value;
}

/// How many numbers a window's `service` holds.
pub(crate) fn numbers_in(window: &Value) -> usize {
    fn count(value: &Value) -> usize {
        match value {
            Value::Object(map) => map.values().map(count).sum(),
            Value::Number(_) => 1,
            _ => 0,
        }
    }
    count(&window["service"])
}

/// The same report with the first scenario's determinism failing at tick 3,210.
pub(crate) fn diverging(mut report: Value) -> Value {
    report["scenarios"][0]["determinism"] = json!({
        "holds": false, "fixed_workers": null,
        "runs": [{"workers": 1}, {"workers": 15}],
        "against": [{
            "base_workers": 1, "workers": 15, "same_hashes": false, "same_orders": true,
            "same_numbers": false, "first_differing_tick": 3210, "hashes": null, "orders": null
        }],
        "diagnosis": {
            "base_workers": 1, "workers": 15, "same_hashes": false, "same_orders": false,
            "same_numbers": false, "first_differing_tick": 3209,
            "hashes": {"tick": 3209, "what": "unit count 5 against 6"}, "orders": null
        }
    });
    report
}

pub(crate) fn measured() -> Measured {
    Measured {
        commit: "a".repeat(40),
        date: "2026-11-27".to_owned(),
        seeds: (1..=8).collect(),
        workers: 1,
        measure: "a fixture".to_owned(),
    }
}

/// Nothing measured: every judged number is judged at 25 percent.
pub(crate) fn unmeasured() -> Spread {
    Spread {
        measured: measured(),
        scenarios: BTreeMap::new(),
    }
}

/// Every judged number of the windows of `standard()` measured at `mad_percent` with no range:
/// a tolerance of twice that, never under 5 percent, and the absolute floors of each number.
pub(crate) fn spread(mad_percent: f64) -> Spread {
    let entry = Entry {
        median: 100.0,
        mad_percent,
        min: 100.0,
        max: 100.0,
    };
    let mut scenarios: BTreeMap<String, BTreeMap<String, BTreeMap<String, Entry>>> =
        BTreeMap::new();
    for (scenario, windows) in [
        ("natural", ["early", "middle"].as_slice()),
        ("late", ["late"].as_slice()),
    ] {
        for window in windows {
            for path in judged() {
                scenarios
                    .entry(scenario.to_owned())
                    .or_default()
                    .entry((*window).to_owned())
                    .or_default()
                    .insert(path.to_owned(), entry);
            }
        }
    }
    Spread {
        measured: measured(),
        scenarios,
    }
}

/// `spread(1.0)` with one number measured differently.
pub(crate) fn with_entry(
    mut spread: Spread,
    window: (&str, &str),
    path: &str,
    entry: Entry,
) -> Spread {
    spread
        .scenarios
        .entry(window.0.to_owned())
        .or_default()
        .entry(window.1.to_owned())
        .or_default()
        .insert(path.to_owned(), entry);
    spread
}
