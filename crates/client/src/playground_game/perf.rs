//! Bounded frame telemetry plus optional developer GPU timing.
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function perf_mark(section) { window.aoePerformance?.mark(section); }"
)]
extern "C" {
    pub(super) fn perf_mark(section: u32);
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
