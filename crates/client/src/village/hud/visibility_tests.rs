use super::visible;
use aoe_core::Camera;

#[test]
fn horizontal_hud_rejection_does_not_query_offscreen_height_and_keeps_selection() {
    use std::cell::Cell;
    let camera = Camera::new([0.0, 0.0], [1000.0, 700.0]);
    let queries = Cell::new(0);
    for index in 0..10_000 {
        assert!(
            super::project_if_horizontal(
                camera,
                [10_000.0 + f64::from(index), 0.0],
                [1.0, 1.0],
                [1000.0, 700.0],
                false,
                || {
                    queries.set(queries.get() + 1);
                    [500.0, 350.0]
                },
            )
            .is_none()
        );
    }
    assert_eq!(queries.get(), 0);
    let selected = super::project_if_horizontal(
        camera,
        [10_000.0, 0.0],
        [1.0, 1.0],
        [1000.0, 700.0],
        true,
        || {
            queries.set(queries.get() + 1);
            [9000.0, -8000.0]
        },
    );
    assert_eq!(selected, Some([9000.0, -8000.0]));
    assert_eq!(queries.get(), 1);
}

#[test]
fn horizontal_hud_prefilter_preserves_edges_at_every_ground_height_and_css_scale() {
    for zoom in [0.16, 0.25, 1.0] {
        let mut camera = Camera::new([925.0, 1277.0], [1920.0, 1080.0]);
        camera.zoom = zoom;
        for scale in [[1.0, 1.0], [0.5, 0.75]] {
            let size = [camera.viewport[0] * scale[0], camera.viewport[1] * scale[1]];
            for x in [
                -1000.0,
                -65.0,
                -64.0,
                -63.0,
                0.0,
                size[0],
                size[0] + 64.0,
                size[0] + 65.0,
            ] {
                let world = camera.screen_to_world(aoe_core::ScreenPoint {
                    x: x / scale[0],
                    y: 200.0,
                });
                for height in [-10_000.0, 0.0, 10_000.0] {
                    let point = camera.world_to_screen_at_height(world, height);
                    let point = [point.x * scale[0], point.y * scale[1]];
                    let actual =
                        super::project_if_horizontal(camera, world, scale, size, false, || point)
                            .is_some_and(|p| visible(p, p[1] - 40.0, size, false));
                    assert_eq!(actual, visible(point, point[1] - 40.0, size, false));
                }
            }
        }
    }
}

#[test]
fn hud_culling_keeps_marker_heads_at_the_viewport_edge() {
    let size = [1000.0, 700.0];
    assert!(visible([500.0, 900.0], 690.0, size, false));
    assert!(visible([-32.0, 200.0], 150.0, size, false));
    assert!(!visible([1100.0, 200.0], 150.0, size, false));
    assert!(!visible([500.0, 900.0], 800.0, size, false));
}

#[test]
fn offscreen_selection_stays_available_without_retaining_the_world() {
    let size = [1000.0, 700.0];
    for point in [[-10000.0, 200.0], [500.0, 10000.0], [10000.0, -10000.0]] {
        assert!(!visible(point, point[1] - 40.0, size, false));
        assert!(visible(point, point[1] - 40.0, size, true));
    }
    assert!(!visible([f64::NAN, 200.0], 160.0, size, false));
}

#[test]
fn construction_label_gutter_keeps_the_seventy_pixel_bar_and_raised_text() {
    let size = [1280.0, 720.0];
    for point in [[-34.0, 300.0], [1314.0, 300.0], [640.0, 739.0]] {
        assert!(visible(point, point[1] - 24.0, size, false));
    }
    for point in [[-500.0, 300.0], [1800.0, 300.0], [640.0, 1000.0]] {
        assert!(!visible(point, point[1] - 24.0, size, false));
    }
}

#[test]
fn resource_projection_skips_horizontal_misses_but_preserves_highlights_and_exact_view_edges() {
    use std::cell::Cell;
    let queries = Cell::new(0);
    let camera = Camera::new([0.0, 0.0], [1000.0, 700.0]);
    let offscreen = [10_000.0, 0.0];
    assert!(
        super::resource_projection(camera, offscreen, false, || {
            queries.set(queries.get() + 1);
            aoe_core::ScreenPoint {
                x: 9000.0,
                y: -8000.0,
            }
        })
        .is_none()
    );
    assert_eq!(queries.get(), 0);
    let highlighted = super::resource_projection(camera, offscreen, true, || {
        queries.set(queries.get() + 1);
        aoe_core::ScreenPoint {
            x: 9000.0,
            y: -8000.0,
        }
    });
    assert!(highlighted.is_some());
    if let Some((point, visible)) = highlighted {
        assert_eq!([point.x, point.y], [9000.0, -8000.0]);
        assert!(!visible);
    }
    assert_eq!(queries.get(), 1);
    for zoom in [0.16, 0.25, 1.0] {
        let mut camera = Camera::new([925.0, 1277.0], [1920.0, 1080.0]);
        camera.zoom = zoom;
        for x in [-1000.0, -1.0, 0.0, 1.0, 1919.0, 1920.0, 1921.0] {
            let world = camera.screen_to_world(aoe_core::ScreenPoint { x, y: 200.0 });
            for height in [-10_000.0, 0.0, 10_000.0] {
                let point = camera.world_to_screen_at_height(world, height);
                let visible = point.x >= 0.0
                    && point.y >= 0.0
                    && point.x < camera.viewport[0]
                    && point.y < camera.viewport[1];
                for highlighted in [false, true] {
                    let actual = super::resource_projection(camera, world, highlighted, || point);
                    assert_eq!(actual.is_some(), highlighted || visible);
                    if let Some((p, actual_visible)) = actual {
                        assert_eq!([p.x, p.y], [point.x, point.y]);
                        assert_eq!(actual_visible, visible);
                    }
                }
            }
        }
    }
}
