//! Per-chunk retry state: bounded storage and a capped request rate.
#[derive(Clone, Copy, Default)]
pub(super) struct Retry {
    pub attempts: u8,
    pub until: f64,
}
impl Retry {
    pub fn fail(&mut self, now: f64) {
        let shift = self.attempts.min(5);
        self.until = now + f64::from(250_u32 << shift);
        self.attempts = (shift + 1).min(5);
    }
}
pub(super) fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}
