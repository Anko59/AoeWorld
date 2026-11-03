//! Bounded strategic presentation shared by every renderer backend.
use super::Client;
use crate::strategic::{Lod, focus_zoom, lod_for_viewport};
use aoe_core::ScreenPoint;
use wasm_bindgen::prelude::wasm_bindgen;

#[path = "strategic/data.rs"]
mod data;
#[cfg(test)]
pub(super) use data::army_record;
pub(super) use data::update;

#[wasm_bindgen(inline_js = "
export function strategic_pick(point) {
  const world = window.aoeNavigation?.strategic?.pick(point[0], point[1]);
  if (world) point.set(world); else point.fill(NaN);
}
export function strategic_frame(state) { return window.aoeNavigation?.strategic?.frame(state) ?? false; }
export function strategic_present() { window.aoeNavigation?.strategic?.present(); }
export function strategic_entity(x, y) { return window.aoeNavigation?.strategic?.entity(x, y) ?? NaN; }
export function strategic_depleted(ids, begin, complete) { window.aoeNavigation?.strategic?.depleted(ids, begin, complete); }
")]
extern "C" {
    fn strategic_frame(state: &[f64]) -> bool;
    fn strategic_pick(point: &mut [f64]);
    fn strategic_present();
    fn strategic_entity(x: f64, y: f64) -> f64;
    pub(super) fn strategic_depleted(ids: &[f64], begin: bool, complete: bool);
}

pub(super) fn level(client: &Client) -> Lod {
    lod_for_viewport(client.camera.zoom, client.camera.viewport)
}

pub(super) fn active(client: &Client) -> bool {
    level(client) != Lod::Sprites
}

pub(super) fn render(client: &mut Client) -> bool {
    let level = level(client);
    let state = [
        f64::from(client.config.width_tiles),
        f64::from(client.config.height_tiles),
        client.camera.center[0],
        client.camera.center[1],
        client.camera.viewport[0],
        client.camera.viewport[1],
        client.camera.zoom,
        f64::from(level as u32),
    ];
    if strategic_frame(&state) {
        client.capture.frame = client.capture.frame.saturating_add(1);
    }
    if level != Lod::Sprites {
        client.rendered_frame.clear();
        true
    } else {
        false
    }
}

pub(super) fn present() {
    strategic_present();
}

pub(super) fn unit(client: &Client, point: ScreenPoint) -> Option<aoe_core::EntityId> {
    let value = strategic_entity(point.x, point.y);
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let id = aoe_core::EntityId(value as u32);
    client
        .units
        .get(&id)
        .filter(|u| {
            u.hp > 0
                && (u.player == super::village::owner(client)
                    || client
                        .fog
                        .state(u.position.as_tiles().map(|p| p.floor() as i32))
                        == 2)
        })
        .map(|_| id)
}

pub(super) fn focus(client: &mut Client, point: ScreenPoint) -> bool {
    if !active(client) {
        return false;
    }
    let entity = strategic_entity(point.x, point.y);
    if entity.is_finite() {
        if entity >= 0.0 {
            if let Some(id) = unit(client, point) {
                client.selected.units(vec![id]);
            }
        } else {
            let id = aoe_core::EntityId((-entity - 1.0) as u32);
            if client.village.buildings.iter().any(|b| b.id == id) {
                client.selected.building(id);
            }
        }
        return true;
    }
    let mut world = [point.x, point.y];
    strategic_pick(&mut world);
    if !world.iter().all(|value| value.is_finite()) {
        return false;
    }
    client.camera.zoom = focus_zoom(client.camera.zoom, client.camera.viewport);
    super::map::center_on_world(client, world);
    client.camera = client.camera.clamp_center(client.config);
    client.pointer = None;
    super::subscribe(client);
    true
}

pub(super) fn screen_position(client: &Client, world: [f64; 2]) -> ScreenPoint {
    client
        .camera
        .world_to_screen_at_height(world, client.camera.focus_elevation_meters)
}

// Overview terrain is this plane, including unloaded tiles. Orders still use
// the ordinary authoritative server validation after projection.
pub(super) fn world_at_screen(client: &Client, screen: ScreenPoint) -> [f64; 2] {
    client
        .camera
        .screen_to_world_at_height(screen, client.camera.focus_elevation_meters)
}
