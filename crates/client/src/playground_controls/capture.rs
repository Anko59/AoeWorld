//! Camera links and cheap presentation evidence, independent of gameplay authority.
use super::{Client, map};
use aoe_core::{Camera, ScreenPoint, WorldConfig};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

pub(crate) struct Fragment(pub [f64; 4]);

impl Fragment {
    pub(super) fn has_camera(&self) -> bool {
        self.0[..3].iter().any(|value| value.is_finite())
    }

    pub fn renderer(&self) -> Option<&'static str> {
        match self.0[3] as u32 {
            1 => Some("webgpu"),
            2 => Some("webgl2"),
            3 => Some("canvas"),
            _ => None,
        }
    }

    pub fn apply(&self, mut camera: Camera, config: WorldConfig) -> Camera {
        if !self.has_camera() {
            return camera;
        }
        if self.0[2].is_finite() {
            camera = camera.zoom_around(
                ScreenPoint {
                    x: camera.viewport[0] / 2.0,
                    y: camera.viewport[1] / 2.0,
                },
                self.0[2],
            );
        }
        for axis in 0..2 {
            if self.0[axis].is_finite() {
                let extent = [config.width_tiles, config.height_tiles][axis];
                camera.center[axis] = self.0[axis].clamp(0.0, f64::from(extent) - 1.0);
            }
        }
        camera.clamp_center(config)
    }
}

pub(super) fn current() -> Fragment {
    let mut values = [f64::NAN, f64::NAN, f64::NAN, 0.0];
    super::controls::navigation::camera_fragment(&mut values);
    Fragment(values)
}

fn apply_current(client: &mut Client) {
    let fragment = current();
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
        failures: &[(i32, i32)],
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
    // Only the current, bounded visible demand survives settle_counts pruning.
    pub failures: Vec<(i32, i32)>,
    pub frame: u64,
}

impl Capture {
    pub fn chunk_received(&mut self, coordinate: (i32, i32), failed: bool) {
        if !failed {
            self.failures.retain(|key| *key != coordinate);
        } else if !self.failures.contains(&coordinate) {
            self.failures.push(coordinate);
        }
    }
}

pub(super) fn update(client: &mut Client) {
    let counts = map::heights::settle_counts(client);
    super::controls::navigation::capture_update(
        &client.canvas,
        counts.pending,
        counts.failed,
        client.capture.frame,
    );
}
