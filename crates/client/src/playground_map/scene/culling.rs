/// Reject only origins outside every possible body/shadow bound.
pub(crate) fn visible(
    camera: aoe_core::Camera,
    position: [f64; 2],
    height: f64,
    extent: [f64; 2],
) -> bool {
    let point = camera.world_to_screen_at_height(position, height);
    if camera.zoom <= 0.0
        || camera.viewport.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || [point.x, point.y, extent[0], extent[1], camera.zoom]
            .iter()
            .any(|v| !v.is_finite())
    {
        return true;
    }
    point.x >= -extent[0] * camera.zoom
        && point.x <= camera.viewport[0] + extent[0] * camera.zoom
        && point.y >= -extent[1] * camera.zoom
        && point.y <= camera.viewport[1] + extent[1] * camera.zoom
}

#[cfg(test)]
#[path = "tests/culling.rs"]
mod tests;
