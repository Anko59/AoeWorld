//! Camera links and cheap presentation evidence, independent of gameplay authority.
use super::{Client, map};
use aoe_core::{Camera, ScreenPoint, WorldConfig};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

#[derive(Default)]
pub(crate) struct Fragment {
    x: Option<f64>,
    y: Option<f64>,
    zoom: Option<f64>,
    pub renderer: Option<&'static str>,
}

pub(crate) fn parse_fragment(hash: &str) -> Fragment {
    let mut fragment = Fragment::default();
    for part in hash.trim_start_matches('#').split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let number = || value.parse::<f64>().ok().filter(|value| value.is_finite());
        match key {
            "x" => fragment.x = number(),
            "y" => fragment.y = number(),
            "zoom" => fragment.zoom = number(),
            "renderer" => {
                fragment.renderer = match value {
                    "webgpu" => Some("webgpu"),
                    "webgl2" => Some("webgl2"),
                    "canvas" => Some("canvas"),
                    _ => None,
                };
            }
            _ => {}
        }
    }
    fragment
}

impl Fragment {
    fn has_camera(&self) -> bool {
        self.x.is_some() || self.y.is_some() || self.zoom.is_some()
    }

    pub fn apply(&self, mut camera: Camera, config: WorldConfig) -> Camera {
        if !self.has_camera() {
            return camera;
        }
        if let Some(zoom) = self.zoom {
            // Use the same allowed range as wheel zoom, without moving its focus.
            camera = camera.zoom_around(
                ScreenPoint {
                    x: camera.viewport[0] / 2.0,
                    y: camera.viewport[1] / 2.0,
                },
                zoom,
            );
        }
        if let Some(x) = self.x {
            camera.center[0] = x.clamp(0.0, f64::from(config.width_tiles) - 1.0);
        }
        if let Some(y) = self.y {
            camera.center[1] = y.clamp(0.0, f64::from(config.height_tiles) - 1.0);
        }
        camera.clamp_center(config)
    }
}

pub(super) fn current(document: &web_sys::Document) -> Fragment {
    parse_fragment(
        &document
            .location()
            .and_then(|location| location.hash().ok())
            .unwrap_or_default(),
    )
}

fn apply_current(client: &mut Client) {
    let fragment = current(&client.document);
    if !fragment.has_camera() {
        return;
    }
    let camera = fragment.apply(client.camera, client.config);
    client.camera.zoom = camera.zoom;
    // Reuse minimap centering, including its deferred terrain altitude focus.
    map::center_on_world(client, camera.center);
    client.camera = client.camera.clamp_center(client.config);
    client.pointer = None;
}

pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    apply_current(&mut shared.borrow_mut());
    let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        apply_current(&mut shared.borrow_mut());
    });
    web_sys::window()
        .ok_or("No window")?
        .add_event_listener_with_callback("hashchange", callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}

pub(super) fn world_received(client: &mut Client, map_changed: bool) {
    if !client.capture.world_known || map_changed {
        // The first welcome supplies actual dimensions; do not retain an initial
        // clamp against the diagnostic world's size. Reconnects preserve panning.
        apply_current(client);
    }
    client.capture.world_known = true;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChunkCounts {
    pub pending: usize,
    pub failed: usize,
}

impl ChunkCounts {
    pub fn for_visible(
        visible: &[(i32, i32)],
        resident: impl Fn(&(i32, i32)) -> bool,
        failures: &BTreeSet<(i32, i32)>,
    ) -> Self {
        let mut counts = Self::default();
        for coordinate in visible {
            if !resident(coordinate) {
                counts.pending += 1;
                counts.failed += usize::from(failures.contains(coordinate));
            }
        }
        counts
    }
}

#[derive(Default)]
pub(crate) struct Capture {
    world_known: bool,
    pub failures: BTreeSet<(i32, i32)>,
    pub frame: u64,
    last_frame: Option<u64>,
    last_counts: Option<ChunkCounts>,
}

pub(super) fn update(client: &mut Client) {
    let counts = map::heights::settle_counts(client);
    let capture = &mut client.capture;
    if capture.last_counts.map(|value| value.pending) != Some(counts.pending) {
        let _ = client
            .canvas
            .set_attribute("data-chunks-pending", &counts.pending.to_string());
    }
    if capture.last_counts.map(|value| value.failed) != Some(counts.failed) {
        let _ = client
            .canvas
            .set_attribute("data-chunks-failed", &counts.failed.to_string());
    }
    capture.last_counts = Some(counts);
    if capture.last_frame != Some(capture.frame) {
        let _ = client
            .canvas
            .set_attribute("data-frame", &capture.frame.to_string());
        capture.last_frame = Some(capture.frame);
    }
}
