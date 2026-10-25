//! Authoritative queue markers follow camera projection on every backend.
use super::Client;
use crate::playground::game::map;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(inline_js = "export function waypoints_update(v) { window.aoeWaypoints?.(v); }")]
extern "C" {
    fn waypoints_update(values: &[f64]);
}

#[inline(never)]
pub(in super::super) fn present(client: &Client) {
    let mut values = Vec::new();
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    for queue in &client.village.move_queues {
        if !client.selected.units.contains(&queue.entity_id) {
            continue;
        }
        for (index, p) in queue.destinations.iter().enumerate() {
            let point = map::screen_position(client, p.as_tiles());
            values.extend_from_slice(&[
                f64::from(queue.entity_id.0),
                index as f64 + 1.0,
                point.x * scale,
                point.y * scale,
            ]);
        }
    }
    waypoints_update(&values);
}
