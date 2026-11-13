//! Opt-in driver: freezes authority, never part of the normal client artifact.
use super::*;

fn with_client<T>(f: impl FnOnce(&mut Client) -> Result<T, JsValue>) -> Result<T, JsValue> {
    ACTIVE.with(|active| {
        let shared = active
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| JsValue::from_str("client is not initialized"))?;
        f(&mut shared.borrow_mut())
    })
}

#[wasm_bindgen]
pub fn frame_bench_freeze(side: u32) -> Result<(), JsValue> {
    with_client(|client| {
        let side = i32::try_from(side).map_err(|_| JsValue::from_str("map side is too large"))?;
        client.config.width_tiles = side;
        client.config.height_tiles = side;
        client
            .config
            .validate()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        client.connection_callbacks.take();
        client.socket.take();
        client.units.clear();
        client.primary = None;
        client.village = Default::default();
        client.presentation.reset(client.config.tick_hz);
        client.selected.units.clear();
        client.pointer = None;
        Ok(())
    })
}

#[wasm_bindgen]
pub fn frame_bench_settled() -> Result<bool, JsValue> {
    with_client(|client| {
        Ok(client
            .native
            .as_ref()
            .is_none_or(|native| native.benchmark_settled()))
    })
}

#[wasm_bindgen]
pub fn frame_bench_reset_scene() -> Result<(), JsValue> {
    with_client(|client| {
        client.terrain_scene.get_mut().take();
        client.rendered_frame.clear();
        Ok(())
    })
}

#[wasm_bindgen]
pub fn frame_bench_reset_clock() -> Result<(), JsValue> {
    with_client(|client| {
        client.village.last_hud = 0.0;
        client.last_frame = 0.0;
        Ok(())
    })
}

/// Uses the normal animation callback; only its clock/camera are scripted.
#[wasm_bindgen]
pub fn frame_bench_camera(x: f64, y: f64, zoom: f64, force: bool) -> Result<(), JsValue> {
    with_client(|client| {
        if ![x, y, zoom].iter().all(|v| v.is_finite()) || zoom <= 0.0 {
            return Err(JsValue::from_str("invalid benchmark camera"));
        }
        client.camera.zoom = zoom;
        map::center_on_world(client, [x, y]);
        if force {
            client.rendered_frame.invalidate();
        }
        Ok(())
    })
}

/// Controlled streaming-pressure probe, outside scene rendering. Sixteen tiny
/// patches exercise the same batch boundary as NativeFrames::update.
#[wasm_bindgen]
pub fn frame_bench_upload_probe() -> Result<(), JsValue> {
    with_client(|client| {
        client
            .renderer
            .with_game_atlas_updates::<_, String>(|renderer| {
                for _ in 0..16 {
                    renderer.update_game_atlas(7, [2047, 2047], [1, 1], &[0; 4])?;
                }
                Ok(())
            })
            .map_err(|e| JsValue::from_str(&e))
    })
}

#[wasm_bindgen]
pub fn frame_bench_take() -> Vec<u64> {
    aoe_rendering::frame_cost::take()
}
#[wasm_bindgen]
pub fn frame_bench_names() -> String {
    aoe_rendering::frame_cost::NAMES.to_owned()
}
