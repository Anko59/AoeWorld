//! CPU wall spans; no GPU synchronization or per-triangle clock reads.
use wasm_bindgen::prelude::wasm_bindgen;
#[wasm_bindgen(
    inline_js = "export function stage_mark(section) { window.aoeTelemetry?.mark(section); }"
)]
extern "C" {
    fn stage_mark(section: u32);
}
pub(super) struct Stage(u32);
impl Stage {
    pub(super) fn new(index: u32) -> Self {
        stage_mark(index + 7);
        Self(index)
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        stage_mark(self.0 + 14);
    }
}
