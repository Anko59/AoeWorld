//! One order's summary and the table printed at the end of a run.
use super::track::{LIMITS, Member, Stamp};
use aoe_sdk::{EntityId, TileCoord};
use serde_json::{Value, json};

fn percentile(mut values: Vec<f64>, share: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let at = ((values.len() - 1) as f64 * share).round() as usize;
    Some((values[at] * 100.0).round() / 100.0)
}

/// Arrivals within `limit` x ideal on both clocks, or `null` when the run
/// ended before that limit (the share cannot be known yet).
fn arrivals(members: &[Member], order: Stamp, end: Stamp, ideal_ticks: f64) -> Value {
    let mut out = serde_json::Map::new();
    for limit in LIMITS {
        let ticks = limit * ideal_ticks;
        let seconds = ticks / 20.0;
        let ticks_known = (end.tick.saturating_sub(order.tick) as f64) >= ticks;
        let wall_known = end.wall - order.wall >= seconds;
        let by_ticks = members
            .iter()
            .filter(|m| {
                m.arrived
                    .is_some_and(|a| (a.tick - order.tick) as f64 <= ticks)
            })
            .count();
        let by_wall = members
            .iter()
            .filter(|m| m.arrived.is_some_and(|a| a.wall - order.wall <= seconds))
            .count();
        out.insert(
            format!("{limit}x"),
            json!({
                "ticks": ticks_known.then_some(by_ticks),
                "wall": wall_known.then_some(by_wall),
            }),
        );
    }
    Value::Object(out)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn order_json(
    size: usize,
    straight: i32,
    siege: bool,
    ids: &[EntityId],
    probe: &Value,
    to: TileCoord,
    issued: Option<Stamp>,
    result: Option<&str>,
    members: &[Member],
    end: Stamp,
) -> Value {
    let Some(order) = issued else {
        return json!({"size": size, "straight_tiles": straight, "siege": siege,
            "issued": false, "selected": ids.len(), "probe": probe});
    };
    let ideal = probe["ideal_ticks"].as_f64().unwrap_or(0.0);
    let motion: Vec<f64> = members
        .iter()
        .filter_map(|m| m.first_motion.map(|s| s.wall - order.wall))
        .collect();
    let motion_ticks: Vec<f64> = members
        .iter()
        .filter_map(|m| m.first_motion.map(|s| (s.tick - order.tick) as f64))
        .collect();
    let shares: Vec<f64> = members.iter().filter_map(Member::speed_share).collect();
    let stall: u64 = members.iter().map(|m| m.stall_ticks).sum();
    let travel: u64 = members.iter().map(|m| m.travel_ticks).sum();
    let mut finals = serde_json::Map::new();
    for member in members {
        let state = if member.arrived.is_some() {
            "arrived".to_owned()
        } else {
            member.task.clone()
        };
        let slot = finals.entry(state).or_insert(json!(0));
        *slot = json!(slot.as_u64().unwrap_or(0) + 1);
    }
    json!({
        "size": size, "straight_tiles": straight, "siege": siege, "issued": true,
        "selected": ids.len(), "observed": members.len(),
        "to": [to.x, to.y], "result": result, "probe": probe,
        "issued_at": {"tick": order.tick, "wall_s": order.wall},
        "observed_ticks": end.tick.saturating_sub(order.tick),
        "observed_wall_s": end.wall - order.wall,
        "ideal_ticks": ideal,
        "first_motion_wall_s": {"p50": percentile(motion.clone(), 0.5),
            "p95": percentile(motion.clone(), 0.95), "max": percentile(motion, 1.0)},
        "first_motion_ticks": {"p50": percentile(motion_ticks.clone(), 0.5),
            "p95": percentile(motion_ticks, 0.95)},
        "never_moved": members.iter().filter(|m| m.first_motion.is_none()).count(),
        "dropped": members.iter().filter(|m| m.dropped.is_some()).count(),
        "dead": members.iter().filter(|m| m.gone.is_some()).count(),
        "speed_share": {"p50": percentile(shares.clone(), 0.5), "p10": percentile(shares, 0.1)},
        "stall_share": if travel > 0 { stall as f64 / travel as f64 } else { 0.0 },
        "arrivals": arrivals(members, order, end, ideal),
        "arrived_by_end": members.iter().filter(|m| m.arrived.is_some()).count(),
        "final_states": finals,
        "final_distance_tiles": {"p50": percentile(members.iter().map(|m| m.distance).collect(), 0.5)},
        "members": members.iter().map(|m| m.json(order)).collect::<Vec<_>>(),
    })
}

fn cell(value: &Value, size: u64) -> String {
    value.as_u64().map_or("-".to_owned(), |n| {
        format!("{:.0}%", 100.0 * n as f64 / size.max(1) as f64)
    })
}

/// Prints one row per order: what an owner would see of it.
pub(in super::super) fn print_table(label: &str, march: &Value) {
    let at = &march["scheduler_at_march"];
    println!(
        "\nmarch probe `{label}` (AIs {}): at the orders waiting={} without_planner={} oldest_wait_ticks={} fair_work={}",
        march["ai"],
        at["waiting"],
        at["waiting_without_planner"],
        at["oldest_wait_ticks"],
        at["fair_work"]
    );
    let ends = &march["scheduler_at_end"]["attribution"]["group_ends"];
    println!("group endings by reason (whole match, AIs included): {ends}");
    println!(
        "| size | tiles | route | ideal s | result | moved p50/max s | dropped | 1.3x t/w | 2x t/w | 3x t/w | end | speed p50 | stall | final |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for order in march["orders"].as_array().into_iter().flatten() {
        let n = order["observed"].as_u64().unwrap_or(0);
        let a = &order["arrivals"];
        let pair = |k: &str| format!("{}/{}", cell(&a[k]["ticks"], n), cell(&a[k]["wall"], n));
        println!(
            "| {}{} | {} | {:.0} {} | {:.0} | {} | {}/{} | {} | {} | {} | {} | {} | {} | {:.0}% | {} |",
            n,
            if order["siege"] == true { " siege" } else { "" },
            order["straight_tiles"],
            order["probe"]["length_tiles"].as_f64().unwrap_or(0.0),
            if order["probe"]["reachable"] == true {
                "ok"
            } else {
                "UNPROVEN"
            },
            order["ideal_ticks"].as_f64().unwrap_or(0.0) / 20.0,
            order["result"].as_str().unwrap_or("-"),
            order["first_motion_wall_s"]["p50"],
            order["first_motion_wall_s"]["max"],
            order["dropped"],
            pair("1.3x"),
            pair("2x"),
            pair("3x"),
            cell(&order["arrived_by_end"], n),
            order["speed_share"]["p50"],
            order["stall_share"].as_f64().unwrap_or(0.0) * 100.0,
            order["final_states"],
        );
    }
}
