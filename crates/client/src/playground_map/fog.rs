use super::Client;
use aoe_protocol::FogState;
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

pub(in super::super) fn minimap(_client: &Client, state: &FogState) {
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
