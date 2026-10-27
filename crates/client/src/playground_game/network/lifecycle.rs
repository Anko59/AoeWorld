//! Persistent connection diagnostics and world replacement.
use super::*;

pub(super) fn record_close(client: &mut Client, event: &Event) {
    let reason = js_sys::Reflect::get(&event, &JsValue::from_str("reason"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "The connection to the server was lost.".to_owned());
    match &mut client.last_error {
        Some(error) if !error.contains(&reason) => {
            error.push(' ');
            error.push_str(&reason);
        }
        Some(_) => {}
        None => client.last_error = Some(reason),
    }
}

pub(super) fn reconnect(client: &mut Client, status: &str) {
    client.status = status.to_owned();
    client.last_error = Some(status.to_owned());
    client.resources.clear();
    if let Some(socket) = &client.socket {
        let _ = socket.close();
    }
}

// The pieces of a connection's world that a new Welcome or a WorldReset drops.
pub(super) fn clear_world(client: &mut Client) {
    client.terrain_scene.get_mut().take();
    client.resources.clear();
    client.fog.clear();
    if !client.surface_fixture {
        map::clear_terrain_cache(client);
    }
    client.units.clear();
    client.village = Default::default();
    client.selected = Default::default();
    let tick_hz = client.config.tick_hz;
    client.presentation.reset(tick_hz);
    client.rendered_frame.clear();
}
