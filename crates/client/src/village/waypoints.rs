//! Authoritative queue markers follow camera projection on every backend.
use super::Client;
use crate::playground::game::{map, position_of};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function waypoints_update(v) { window.aoeWaypoints?.(v); } export function order_ping(x, y) { window.aoeOrderPing?.(x, y); } export function clear_order_pings() { document.querySelectorAll('.order-ping,[data-minimap-ping]').forEach(node => node.remove()); }"
)]
extern "C" {
    fn waypoints_update(values: &[f64]);
    fn order_ping(x: f64, y: f64);
    pub(super) fn clear_order_pings();
}

/// A short-lived ring at an attack-move destination.
pub(in super::super) fn ping(client: &Client, destination: aoe_core::WorldPosition) {
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    let point = map::screen_position(client, destination.as_tiles());
    order_ping(point.x * scale, point.y * scale);
}

#[inline(never)]
pub(in super::super) fn present(client: &Client) {
    if client.village.move_queues.is_empty() || client.selected.units.is_empty() {
        waypoints_update(&[]);
        return;
    }
    let mut values = Vec::new();
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    let selected: std::collections::HashSet<_> = client.selected.units.iter().copied().collect();
    let mut shared = std::collections::HashMap::new();
    for queue in &client.village.move_queues {
        if queue.destinations.is_empty() || !selected.contains(&queue.entity_id) {
            continue;
        }
        let Some(unit) = client
            .units
            .get(&queue.entity_id)
            .filter(|unit| unit.hp > 0)
        else {
            continue;
        };
        let start = map::screen_position(client, position_of(client, unit));
        let id = f64::from(queue.entity_id.0);
        // Index zero starts the first leg without drawing a waypoint marker.
        values.extend_from_slice(&[id, 0.0, start.x * scale, start.y * scale, 0.0, 0.0]);
        for (index, waypoint) in queue.destinations.iter().enumerate() {
            let order_id = queue.order_ids[index];
            let project = || map::screen_position(client, waypoint.as_tiles());
            let point = if order_id == 0 {
                project()
            } else {
                *shared.entry(order_id).or_insert_with(project)
            };
            values.extend_from_slice(&[
                id,
                index as f64 + 1.0,
                point.x * scale,
                point.y * scale,
                f64::from(u8::from(queue.is_attack_move(index))),
                f64::from(order_id),
            ]);
        }
    }
    waypoints_update(&values);
}
