//! Bounded frame telemetry plus optional developer GPU timing.
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function perf_mark(section) { window.aoePerformance?.mark(section); }"
)]
extern "C" {
    pub(super) fn perf_mark(section: u32);
}

#[wasm_bindgen(
    inline_js = "export function perf_view(x, y) { return window.aoeTelemetry?.view?.(x, y) ?? false; }
export function perf_publish(units, effect, plain, contours, occluders, span, lost, restored) {
    window.aoeTelemetry?.publish?.(units, effect, plain, contours, occluders, span, lost, restored);
}"
)]
extern "C" {
    pub(super) fn perf_view(x: f64, y: f64) -> bool;
    fn perf_publish(
        units: u32,
        effect: u32,
        plain: u32,
        contours: u32,
        occluders: u32,
        span: f32,
        lost: u32,
        restored: u32,
    );
}

pub(super) fn publish(client: &super::Client) {
    let units = client
        .rendered_frame
        .painted_units()
        .iter()
        .filter(|sprite| client.units.get(&sprite.id).is_some_and(|unit| unit.hp > 0))
        .count();
    let stats = client.renderer.contour_stats();
    perf_publish(
        units as u32,
        stats.effect_frames,
        stats.plain_frames,
        stats.contour_instances,
        stats.occluder_instances,
        stats.depth_span,
        stats.context_lost,
        stats.context_restored,
    );
}

pub(super) fn decode_message(
    bytes: &[u8],
) -> Result<aoe_protocol::GameplayServerMessage, aoe_protocol::GameplayError> {
    #[cfg(feature = "frame-bench")]
    let _decode = Scope::new(47);
    let message = aoe_protocol::decode_gameplay_server(bytes);
    // Tag the enclosing decode/apply span, including superseded snapshots.
    #[cfg(feature = "frame-bench")]
    if matches!(
        message,
        Ok(aoe_protocol::GameplayServerMessage::Snapshot { .. })
    ) {
        perf_mark(22);
    }
    message
}

pub(super) fn decode() -> (Scope, Scope) {
    (Scope::new(1), Scope::stage(0))
}

pub(super) struct Scope(u32);

impl Scope {
    pub(super) fn stage(section: u32) -> Self {
        Self::new(section + 7)
    }

    pub(super) fn new(section: u32) -> Self {
        perf_mark(section);
        Self(section)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        perf_mark(self.0 + if self.0 >= 7 { 7 } else { 3 });
    }
}
