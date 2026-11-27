//! Camera motion and view-subscription geometry, independent of browser clocks.
use aoe_core::TileRect;

#[derive(Default)]
pub(crate) struct PanMotion {
    previous: Option<([f64; 2], f64)>,
}

impl PanMotion {
    pub(crate) fn observe(&mut self, center: [f64; 2], time: f64) -> [f64; 2] {
        let Some((previous, previous_time)) = self.previous.replace((center, time)) else {
            return center;
        };
        let elapsed = time - previous_time;
        let delta = [center[0] - previous[0], center[1] - previous[1]];
        if elapsed <= 0.0 || elapsed > 500.0 || delta.iter().any(|d| d.abs() > 96.0) {
            return center;
        }
        let ahead = 300.0 / elapsed;
        [0, 1].map(|axis| center[axis] + (delta[axis] * ahead).clamp(-96.0, 96.0))
    }
}

pub(crate) fn contains(outer: TileRect, inner: TileRect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

/// Pre-subscribe a margin on all sides, so an ordinary camera move can stay
/// within the current stream, including a reversal. Keep the existing wire limit.
pub(crate) fn guarded_subscription(visible: TileRect, [width, height]: [i32; 2]) -> TileRect {
    const GUARD: i32 = 64;
    let extent = [visible.width(), visible.height()]
        .map(|axis| (axis + 2 * GUARD).min(aoe_protocol::MAX_SUBSCRIPTION_TILES));
    TileRect::from_xywh(
        visible.min.x - (extent[0] - visible.width()) / 2,
        visible.min.y - (extent[1] - visible.height()) / 2,
        extent[0],
        extent[1],
    )
    .clamp(width, height)
}

#[path = "view_tests.rs"]
#[cfg(test)]
mod tests;
