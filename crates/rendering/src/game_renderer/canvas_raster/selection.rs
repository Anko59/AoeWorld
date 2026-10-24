use super::write_fragment;

pub(in crate::game_renderer) fn raster_selection(
    sprite: crate::web::Sprite,
    layer_depth: f64,
    viewport: [f64; 2],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
) {
    let center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
    let center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
    let radius_x =
        f64::from(sprite.radius[0]) * viewport[0] / if sprite.pages[3] == 2 { 2.0 } else { 1.0 };
    let radius_y =
        f64::from(sprite.radius[1]) * viewport[1] / if sprite.pages[3] == 2 { 2.0 } else { 1.0 };
    if radius_x <= 0.0 || radius_y <= 0.0 {
        return;
    }
    let rgba = sprite
        .color
        .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8);
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            let dx = (f64::from(x) + 0.5 - center_x) / radius_x;
            let dy = (f64::from(y) + 0.5 - center_y) / radius_y;
            let inside = if sprite.pages[3] == 2 {
                let px = f64::from(x) + 0.5 - center_x;
                let py = f64::from(y) + 0.5 - center_y;
                let rx = f64::from(sprite.uv[0]);
                let ry = f64::from(sprite.uv[1]);
                let d = ((px / rx).powi(2) + (py / ry).powi(2) - 1.0).abs()
                    / (2.0 * (px / (rx * rx)).hypot(py / (ry * ry))).max(0.0001);
                d <= f64::from(sprite.uv[2]) * 0.5
            } else {
                dx * dx + dy * dy <= 1.0
            };
            if inside {
                write_fragment(
                    x,
                    y,
                    width,
                    crate::game_grid::selection_depth_at(
                        &sprite,
                        layer_depth,
                        f64::from(y) + 0.5,
                        viewport[1],
                    ),
                    rgba,
                    color,
                    depth,
                );
            }
        }
    }
}
