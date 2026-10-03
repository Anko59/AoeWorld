use super::{Client, map, pan, pixel_scale, position_at};
use wasm_bindgen::prelude::wasm_bindgen;

// UI navigation stays bounded and never widens the detailed terrain subscription.
#[wasm_bindgen(inline_js = "
export function navigation_frame(state) {
  const output = window.aoeNavigation?.frame(state.subarray(0, 11));
  if (output) state.set(output, 11);
}
export function navigation_map(hash) { window.aoeNavigation?.map(hash); }
export function gameplay_key(event) {
  if (event.repeat || event.ctrlKey || event.altKey || event.metaKey ||
      (event.target instanceof Element && event.target.closest('input,select,textarea,[contenteditable]'))) return 0;
  const action = event.key === 'Escape' ? 1 : event.key === 'g' || event.key === 'G' ? 2 : event.key === 'Home' ? 3 : 0;
  if (action) event.preventDefault();
  return action;
}
")]
extern "C" {
    fn navigation_frame(state: &mut [f64]);
    fn navigation_map(hash: &[u8]);
    pub(super) fn gameplay_key(event: &web_sys::Event) -> u32;
}

pub(super) fn map_changed(hash: Option<[u8; 32]>) {
    navigation_map(hash.as_ref().map_or(&[], |hash| hash.as_slice()));
}

#[inline(never)]
pub(super) fn frame(client: &mut Client, delta_ms: f64) {
    let primary = client
        .primary
        .map_or([f64::NAN; 2], |id| position_at(client, id));
    let mut input = [
        f64::from(client.config.width_tiles),
        f64::from(client.config.height_tiles),
        client.camera.center[0],
        client.camera.center[1],
        primary[0],
        primary[1],
        client.camera.viewport[0],
        client.camera.viewport[1],
        client.camera.zoom,
        delta_ms,
        pixel_scale(client),
        0.0,
        0.0,
        f64::NAN,
        f64::NAN,
    ];
    navigation_frame(&mut input);
    let destination = [input[13], input[14]];
    if destination.iter().all(|value| value.is_finite()) {
        let destination = [
            destination[0].clamp(0.0, f64::from(client.config.width_tiles) - 1.0),
            destination[1].clamp(0.0, f64::from(client.config.height_tiles) - 1.0),
        ];
        map::center_on_world(client, destination);
        client.camera = client.camera.clamp_center(client.config);
        client.pointer = None;
    }
    let delta = [input[11], input[12]];
    if delta.iter().all(|value| value.is_finite()) && delta != [0.0; 2] {
        pan(client, delta);
    }
}
