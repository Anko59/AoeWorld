//! Generated current DE graphic topology; no original source pixels.
#[derive(Clone, Copy)]
pub(super) struct Graphic {
    pub stem: &'static str,
    pub cells: u16,
    pub seconds: f32,
    pub replay_delay: f32,
    pub angles: u16,
    pub stride: u16,
}
#[path = "native/generated.rs"]
mod generated;
pub(super) const fn get(id: u32) -> Option<Graphic> {
    generated::get(id)
}
const fn lookup(entries: &[(u32, Graphic)], id: u32) -> Option<Graphic> {
    let mut lo = 0;
    let mut hi = entries.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let (key, value) = entries[mid];
        if key == id {
            return Some(value);
        }
        if key < id {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    None
}
