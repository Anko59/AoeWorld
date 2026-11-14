//! Order demand by issuer. Rates are per recorded tick; the counters start empty at restore,
//! so cumulative values are the replay window's own.
use aoe_simulation::{GameWorld, ISSUER_SPANS};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const NAMES: [&str; 8] = [
    "player",
    "python_ai",
    "auto_scout",
    "steward",
    "vassal",
    "general",
    "rally_automation",
    "unattributed",
];

pub(super) fn report(
    world: &GameWorld,
    kinds: &BTreeMap<String, (u64, u64)>,
    census: &[u64; 8],
    ticks: usize,
) -> Value {
    let ticks = ticks.max(1) as f64;
    let work = world.order_route_work();
    let mut issuers = serde_json::Map::new();
    for (i, name) in NAMES.iter().enumerate() {
        let mut row = match world.order_demand().get(i) {
            Some(demand) => serde_json::to_value(demand).unwrap_or_default(),
            None => json!({}),
        };
        row["route_work"] = json!({"issue": work[i][0], "fair": work[i][1], "ahead": work[i][2]});
        row["mover_ticks_per_tick"] = json!(census[i] as f64 / ticks);
        issuers.insert((*name).to_owned(), row);
    }
    let redundancy: BTreeMap<String, Value> = world
        .order_redundancy_sources()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    let commands: BTreeMap<&String, Value> = kinds
        .iter()
        .map(|(k, (n, i))| (k, json!({"commands": n, "instructions": i, "instructions_per_command": *i as f64 / (*n).max(1) as f64})))
        .collect();
    json!({
        "ticks": ticks, "issuers": issuers, "redundancy_sources": redundancy,
        "commands_by_issuer_kind_result": commands,
        "issuer_spans": ISSUER_SPANS,
        "note": "recorded commands replay as python_ai (the human seat sent none); route_work is planner work units charged to the issuer of the unit's latest accepted order; mover census is units with an unfinished goal per tick by that issuer",
    })
}
