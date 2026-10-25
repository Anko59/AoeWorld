//! Browser-side clocks and bounded samples; no clock reads when disabled.
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function perf_mark(section) { window.aoePerformance?.mark(section); }"
)]
extern "C" {
    pub(super) fn perf_mark(section: u32);
}

pub(super) struct Scope(u32);

impl Scope {
    pub(super) fn new(section: u32) -> Self {
        perf_mark(section);
        Self(section)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        perf_mark(self.0 + 3);
    }
}
