//! Bound HUD projection records without dropping offscreen selection details.
const GUTTER: f64 = 64.0;

fn horizontal(x: f64, width: f64) -> bool {
    x.is_finite() && x >= -GUTTER && x <= width + GUTTER
}

/// Height changes only projected Y. Reject impossible X before asking for the
/// displayed ground contact, while selected actors always keep their details.
pub(crate) fn project_if_horizontal(
    camera: aoe_core::Camera,
    world: [f64; 2],
    scale: [f64; 2],
    size: [f64; 2],
    selected: bool,
    project: impl FnOnce() -> [f64; 2],
) -> Option<[f64; 2]> {
    if selected
        || horizontal(
            camera.world_to_screen_at_height(world, 0.0).x * scale[0],
            size[0],
        )
    {
        Some(project())
    } else {
        None
    }
}

pub(crate) fn visible(point: [f64; 2], head: f64, size: [f64; 2], selected: bool) -> bool {
    if selected {
        return true;
    }
    point.iter().all(|v| v.is_finite())
        && head.is_finite()
        && horizontal(point[0], size[0])
        && point[1].max(head) >= -GUTTER
        && point[1].min(head) <= size[1] + GUTTER
}

#[cfg(test)]
#[path = "visibility_tests.rs"]
mod tests;

/// Resource markers use the exact half-open viewport. Highlights retain their
/// projected coordinates even outside it; nearest-resource hints are separate.
pub(crate) fn resource_projection(
    camera: aoe_core::Camera,
    world: [f64; 2],
    highlighted: bool,
    project: impl FnOnce() -> aoe_core::ScreenPoint,
) -> Option<(aoe_core::ScreenPoint, bool)> {
    let x = camera.world_to_screen(world).x;
    if !highlighted && !(x >= 0.0 && x < camera.viewport[0]) {
        return None;
    }
    let point = project();
    let visible = point.x >= 0.0
        && point.y >= 0.0
        && point.x < camera.viewport[0]
        && point.y < camera.viewport[1];
    (highlighted || visible).then_some((point, visible))
}
