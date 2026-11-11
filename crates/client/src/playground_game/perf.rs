//! Bounded frame telemetry plus optional developer GPU timing.
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function perf_mark(section) { window.aoePerformance?.mark(section); }"
)]
extern "C" {
    pub(super) fn perf_mark(section: u32);
}

#[wasm_bindgen(
    inline_js = "export function perf_contour(effect, plain, contours, occluders, span, lost, restored) {
    window.aoeTelemetry?.contour(effect, plain, contours, occluders, span, lost, restored);
}"
)]
extern "C" {
    fn perf_contour(
        effect: u32,
        plain: u32,
        contours: u32,
        occluders: u32,
        span: f32,
        lost: u32,
        restored: u32,
    );
}

/// Contour observations of the last submitted frame; one cheap call per frame.
pub(super) fn contour(stats: aoe_rendering::ContourStats) {
    perf_contour(
        stats.effect_frames,
        stats.plain_frames,
        stats.contour_instances,
        stats.occluder_instances,
        stats.depth_span,
        stats.context_lost,
        stats.context_restored,
    );
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
