use super::{Client, map, pan, pixel_scale, position_at};
use wasm_bindgen::prelude::wasm_bindgen;

// UI navigation stays bounded and never widens the detailed terrain subscription.
#[wasm_bindgen(inline_js = "
export function overview_active() { return window.aoeNavigation?.overview?.() ?? false; }
export function navigation_frame(state) {
  const output = window.aoeNavigation?.frame(state.subarray(0, 11));
  if (output) state.set(output, 11);
}
export function navigation_present(state) { window.aoeNavigation?.present(state); }
export function camera_fragment(state) {
  const values = window.aoeNavigation?.fragment();
  if (values) state.set(values);
}
const presentations = new WeakMap();
export function capture_update(canvas, pending, failed, frame) {
  const previous = presentations.get(canvas) ?? [];
  const values = [pending, failed, frame];
  const names = ['data-chunks-pending', 'data-chunks-failed', 'data-frame'];
  for (let index = 0; index < values.length; index++) {
    if (previous[index] !== values[index]) canvas.setAttribute(names[index], String(values[index]));
  }
  presentations.set(canvas, values);
}
export function navigation_map(hash) { window.aoeNavigation?.clearGroups(); window.aoeNavigation?.map(hash); }
export function control_group(action, selected, positions) {
  return Uint32Array.from(window.aoeNavigation?.group(action, selected, positions) ?? []);
}
export function groups_update(positions) { window.aoeNavigation?.groupsUpdate(positions); }
export function group_members() { return window.aoeNavigation?.groupMembers() ?? 0; }
export function gameplay_key(event) {
  if (event.type === 'blur') return 6;
  if (event.type === 'keyup') return event.key === 'Shift' ? 6 : 0;
  if (document.querySelector('dialog[open]') || event.repeat || event.altKey || event.metaKey ||
      (event.target instanceof Element && event.target.closest('input:not([type=button]):not([type=checkbox]):not([type=radio]),textarea,select,[contenteditable]'))) return 0;
  if (/^Digit[0-9]$/.test(event.code)) {
    event.preventDefault();
    return Number(event.code.slice(5)) + (event.ctrlKey ? 20 : event.shiftKey ? 30 : 10);
  }
  if (event.ctrlKey) return 0;
  if (window.aoeBuildings?.key(event)) return 0;
  const action = event.key === 'Escape' ? 1 : event.key === 'g' || event.key === 'G' ? 2 : event.key === 'Home' ? 3 : event.key.toLowerCase() === 'q' ? 4 : event.key.toLowerCase() === 'h' ? 5 : event.key === '.' ? 7 : event.key === ',' ? 8 : 0;
  if (action) event.preventDefault();
  return action;
}
")]
extern "C" {
    fn navigation_frame(state: &mut [f64]);
    fn navigation_present(state: &[f64]);
    pub(in super::super) fn overview_active() -> bool;
    pub(in super::super) fn camera_fragment(state: &mut [f64]);
    pub(in super::super) fn capture_update(
        canvas: &web_sys::HtmlCanvasElement,
        pending: usize,
        failed: usize,
        frame: u64,
    );
    fn navigation_map(hash: &[u8]);
    pub(super) fn control_group(
        action: u32,
        selected: &[u32],
        positions: &[f64],
    ) -> js_sys::Uint32Array;
    pub(super) fn groups_update(positions: &[f64]);
    pub(in super::super) fn group_members() -> usize;
    pub(super) fn gameplay_key(event: &web_sys::Event) -> u32;
}

pub(super) fn map_changed(hash: Option<[u8; 32]>) {
    navigation_map(hash.as_ref().map_or(&[], |hash| hash.as_slice()));
}

fn frame_state(client: &Client, delta_ms: f64) -> [f64; 15] {
    let primary = client
        .primary
        .map_or([f64::NAN; 2], |id| position_at(client, id));
    [
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
    ]
}

#[inline(never)]
pub(super) fn frame(client: &mut Client, delta_ms: f64) {
    let mut input = frame_state(client, delta_ms);
    navigation_frame(&mut input);
    let destination = [input[13], input[14]];
    if destination.iter().all(|value| value.is_finite()) {
        let destination = [
            aoe_core::clamp_f64(
                destination[0],
                0.0,
                f64::from(client.config.width_tiles) - 1.0,
            ),
            aoe_core::clamp_f64(
                destination[1],
                0.0,
                f64::from(client.config.height_tiles) - 1.0,
            ),
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

pub(in super::super) fn present(client: &Client) {
    // Publish the same final camera used by this frame, after keyboard,
    // minimap and edge panning have all been applied.
    navigation_present(&frame_state(client, 0.0)[..11]);
}
