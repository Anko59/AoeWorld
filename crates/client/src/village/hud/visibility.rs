//! Bound HUD projection records without dropping offscreen selection details.
pub(crate) fn visible(point: [f64; 2], head: f64, size: [f64; 2], selected: bool) -> bool {
    if selected {
        return true;
    }
    const GUTTER: f64 = 64.0;
    point.iter().all(|v| v.is_finite())
        && head.is_finite()
        && point[0] >= -GUTTER
        && point[0] <= size[0] + GUTTER
        && point[1].max(head) >= -GUTTER
        && point[1].min(head) <= size[1] + GUTTER
}

#[cfg(test)]
#[path = "visibility_tests.rs"]
mod tests;
