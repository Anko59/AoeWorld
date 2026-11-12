/// Native anchors may lie below the image; their small ground target is usable.
pub(crate) fn actor_hit(bounds: [f64; 4], foot: [f64; 2], point: [f64; 2], zoom: f64) -> bool {
    let dx = point[0] - foot[0];
    let dy = point[1] - foot[1];
    (point[0] >= bounds[0]
        && point[0] <= bounds[2]
        && point[1] >= bounds[1]
        && point[1] <= bounds[3])
        || dx * dx + dy * dy <= (5.0 * zoom).powi(2)
}

/// Sprite-body hits need no ground query. A horizontal miss of both the body
/// and the small ground target cannot be repaired by any terrain height.
pub(crate) fn actor_hit_projected(
    bounds: Option<[f64; 4]>,
    foot_x: f64,
    point: [f64; 2],
    zoom: f64,
    project: impl FnOnce() -> [f64; 2],
) -> bool {
    if bounds.is_some_and(|b| {
        point[0] >= b[0] && point[0] <= b[2] && point[1] >= b[1] && point[1] <= b[3]
    }) {
        return true;
    }
    let dx = point[0] - foot_x;
    let fallback_x =
        bounds.is_none() && point[0] >= foot_x - 7.0 * zoom && point[0] <= foot_x + 7.0 * zoom;
    if !fallback_x && dx * dx > (5.0 * zoom).powi(2) {
        return false;
    }
    let foot = project();
    let bounds = bounds.unwrap_or([
        foot[0] - 7.0 * zoom,
        foot[1] - 30.0 * zoom,
        foot[0] + 7.0 * zoom,
        foot[1] + 2.0 * zoom,
    ]);
    actor_hit(bounds, foot, point, zoom)
}
