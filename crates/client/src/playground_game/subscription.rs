//! Plan view changes once and send them from a separate browser task.
use super::*;

const SUBSCRIBE_INTERVAL_MS: f64 = 150.0;
const SUBSCRIBE_RETRY_MS: f64 = 1_100.0;

fn requested_region(client: &Client) -> TileRect {
    let visible = map::terrain_visible_tiles(client);
    let width = visible
        .width()
        .clamp(1, aoe_protocol::MAX_SUBSCRIPTION_TILES);
    let height = visible
        .height()
        .clamp(1, aoe_protocol::MAX_SUBSCRIPTION_TILES);
    TileRect::from_xywh(
        visible.min.x + (visible.width() - width) / 2,
        visible.min.y + (visible.height() - height) / 2,
        width,
        height,
    )
    .clamp(client.config.width_tiles, client.config.height_tiles)
}

fn unit_zoom(client: &Client) -> bool {
    crate::strategic::lod_for_viewport(client.camera.zoom, client.camera.viewport)
        == crate::strategic::Lod::Sprites
}

pub(super) fn subscribed_to_view(client: &Client) -> bool {
    client.sent_unit_zoom == unit_zoom(client)
        && client
            .sent_region
            .is_some_and(|sent| camera_demand::contains(sent, requested_region(client)))
}

pub(super) fn reset(client: &mut Client) {
    client.sent_region = None;
    client.subscription_guard = true;
    client.subscribe_after = 0.0;
}

pub(super) fn retry_refused_view(client: &mut Client, time: f64) {
    client.sent_region = None;
    client.subscribe_after = time + SUBSCRIBE_RETRY_MS;
}

pub(super) fn refuse_over_limit(client: &mut Client, revision: u64) {
    if client.revision != revision || client.sent_region.is_none() {
        return;
    }
    if client.subscription_guard {
        // The server knows about units outside our last snapshot. After a
        // refusal, keep camera requests lean for the rest of this connection.
        client.subscription_guard = false;
        client.sent_region = None;
    } else {
        retry_refused_view(client, now());
    }
}

pub(super) fn subscription_due(client: &Client) -> bool {
    client.primary.is_some()
        && client
            .socket
            .as_ref()
            .is_some_and(|s| s.ready_state() == WebSocket::OPEN)
        && now() >= client.subscribe_after
        && !subscribed_to_view(client)
}

pub(super) fn subscribe(client: &mut Client) {
    #[cfg(feature = "frame-bench")]
    let _subscription = perf::Scope::new(42);
    if !subscription_due(client) {
        return;
    }
    let visible = requested_region(client);
    let region = if client.subscription_guard {
        camera_demand::guarded_subscription(
            visible,
            [client.config.width_tiles, client.config.height_tiles],
        )
    } else {
        visible
    };
    let unit_zoom = unit_zoom(client);
    client.revision = client.revision.saturating_add(1);
    let message = GameplayClientMessage::Subscribe {
        revision: client.revision,
        region,
        unit_zoom,
    };
    if let Ok(bytes) = encode_gameplay_client(&message)
        && network::send(client, &bytes)
    {
        client.sent_region = Some(region);
        client.sent_unit_zoom = unit_zoom;
        // Leave headroom below the server's rolling ten-per-second window.
        client.subscribe_after = now() + SUBSCRIBE_INTERVAL_MS;
    }
}
