use super::Client;
use aoe_protocol::{FogDelta, FogState};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function fog_minimap(disabled, pixels, explored, visible) {
  window.aoeNavigation?.fog?.(disabled, pixels);
  const scene = document.getElementById('scene');
  scene?.setAttribute('data-fog-explored', String(explored));
  scene?.setAttribute('data-fog-visible', String(visible));
  scene?.setAttribute('data-fog-disabled', String(disabled));
}
")]
extern "C" {
    fn fog_minimap(disabled: bool, pixels: &[u32], explored: usize, visible: usize);
}

/// The whole view of the current subscription replaces the one held.
pub(in super::super) fn receive(client: &mut Client, state: FogState) {
    if client.surface_fixture || state.revision != client.revision {
        return;
    }
    minimap(client, &state);
    let was_disabled = client.fog.disabled();
    if client.fog.apply(state) {
        client.rendered_frame.clear();
        if was_disabled != client.fog.disabled() {
            client.terrain_scene.borrow_mut().take();
        }
    }
}

/// Changes to the held view of the current subscription. False when the view
/// they were made for is not held: server and client no longer agree.
pub(in super::super) fn receive_delta(client: &mut Client, delta: &FogDelta) -> bool {
    if client.surface_fixture || delta.revision != client.revision {
        return true;
    }
    if !client.fog.apply_delta(delta) {
        return false;
    }
    if delta.changes_tiles() {
        client.rendered_frame.clear();
    }
    if let Some(view) = client.fog.view() {
        minimap(client, view);
    }
    true
}

fn minimap(_client: &Client, state: &FogState) {
    let pixels = state
        .overview
        .iter()
        .map(|(index, state)| u32::from(*index) | u32::from(*state) << 16)
        .collect::<Vec<_>>();
    fog_minimap(
        state.disabled,
        &pixels,
        state.tiles.iter().filter(|s| **s > 0).count(),
        state.tiles.iter().filter(|s| **s == 2).count(),
    );
}

pub(super) fn uniform_ground(region: aoe_core::TileRect) -> Vec<aoe_rendering::SceneTerrain> {
    let mut tiles = Vec::new();
    for y in region.min.y..region.max.y {
        for x in region.min.x..region.max.x {
            tiles.push(aoe_rendering::SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: aoe_rendering::SceneTerrainSurface::flat(0.0),
                appearance: None,
            });
        }
    }
    tiles
}
