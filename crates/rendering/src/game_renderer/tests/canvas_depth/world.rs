//! CPU raster AND actual Canvas presentation, including owned atlas admission.
use super::*;
use crate::surface_mesh::{ProjectedSurfaceTriangle, world::fixtures as fixture};

fn draw_world(
    renderer: &mut GameRenderer,
    faces: &[ProjectedSurfaceTriangle],
    points: [[u32; 2]; 5],
) -> [[u8; 4]; 5] {
    let GameRenderer::Canvas {
        canvas,
        context,
        source_atlas,
        presentation,
        ..
    } = renderer
    else {
        panic!("Canvas owner required");
    };
    let layers = faces
        .iter()
        .copied()
        .map(|mut face| {
            face.retain_world_texture(source_atlas.world);
            WorldLayer::Surface(face)
        })
        .collect::<Vec<_>>();
    canvas_depth::render_canvas_world(
        canvas,
        context,
        source_atlas,
        presentation,
        &layers,
        test_camera(),
        false,
    )
    .unwrap();
    let presented = context
        .get_image_data(0.0, 0.0, 128.0, 128.0)
        .unwrap()
        .data()
        .0;
    points.map(|[x, y]| {
        let actual = pixel(&presentation.color_buffer, x, y);
        let index = ((y * 128 + x) * 4) as usize;
        assert_eq!(
            &presented[index..index + 4],
            &actual,
            "Canvas failed to present raster byte at {x},{y}"
        );
        actual
    })
}

#[wasm_bindgen_test]
fn canvas_world_native_phase_lod_zoom_alpha_and_replacement_pixels() {
    let mut data = fixture::Fixture::new();
    let mut constructed = data.constructed();
    let (canvas, context) = target_canvas().expect("actual browser Canvas target");
    let mut renderer = GameRenderer::Canvas {
        canvas,
        context,
        atlas: [None, None, None],
        source_atlas: CanvasAtlas::default(),
        presentation: CanvasPresentation::new(128, 128),
    };
    assert_eq!(renderer.world_atlas(), None);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    for case in fixture::cases() {
        let points = fixture::probes(case);
        let mut faces = fixture::faces(&data.art, case, false);
        let expected = points.map(|point| fixture::expected(case, point));
        let coarse = draw_world(&mut renderer, &faces, points);
        let fine = draw_world(
            &mut renderer,
            &fixture::faces(&data.art, case, true),
            points,
        );
        for i in 0..5 {
            fixture::assert_rgba(coarse[i], expected[i]);
            fixture::assert_rgba(fine[i], expected[i]);
            fixture::assert_rgba(fine[i], coarse[i]);
        }
        faces.reverse();
        let reversed = draw_world(&mut renderer, &faces, points);
        let mut other_zoom = case;
        other_zoom.zoom = !case.zoom;
        let zoomed = draw_world(
            &mut renderer,
            &fixture::faces(&data.art, other_zoom, false),
            fixture::probes(other_zoom),
        );
        for i in 0..5 {
            fixture::assert_rgba(reversed[i], coarse[i]);
            fixture::assert_rgba(zoomed[i], coarse[i]);
        }
    }
    let case = fixture::cases()[4];
    let cached = fixture::faces(&data.art, case, false);
    let legacy = fixture::legacy(&cached);
    let old_key = data.art.terrain_world.unwrap();
    renderer.upload_game_atlas(&data.atlas).unwrap();
    assert_eq!(renderer.world_atlas(), None);
    let raw = draw_world(&mut renderer, &cached, fixture::probes(case));
    let old = draw_world(&mut renderer, &legacy, fixture::probes(case));
    assert_eq!(raw, old);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    data.reject_invalid_art(&constructed, |atlas, art| {
        renderer.upload_terrain_atlas(atlas, art)
    });
    assert_eq!(renderer.world_atlas(), Some(old_key));
    assert!(renderer.upload_game_atlas(&[]).is_err());
    assert_eq!(renderer.world_atlas(), None);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    for replacement in 0..3 {
        if replacement < 2 {
            data.corrupt(replacement == 1);
        } else {
            data.layout(true);
            assert_ne!(data.art.terrain_world.unwrap(), old_key);
        }
        renderer.upload_game_atlas(&data.atlas).unwrap();
        let stale = draw_world(&mut renderer, &cached, fixture::probes(case));
        let old = draw_world(&mut renderer, &legacy, fixture::probes(case));
        for i in 0..5 {
            fixture::assert_rgba(stale[i], old[i]);
        }
    }
    drop(constructed);
    constructed = data.constructed();
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    let fresh = draw_world(
        &mut renderer,
        &fixture::faces(&data.art, case, false),
        fixture::probes(case),
    );
    for (actual, point) in fresh.into_iter().zip(fixture::probes(case)) {
        fixture::assert_rgba(actual, fixture::expected(case, point));
    }
    // Exercise the actual production GameRenderer path in addition to cached-layer raster coverage.
    renderer
        .render_prepared_world(
            &data.art,
            &[],
            &fixture::faces(&data.art, case, false),
            &[],
            &[],
            test_camera(),
            0,
            None,
        )
        .unwrap();
    let GameRenderer::Canvas { context, .. } = &renderer else {
        unreachable!();
    };
    let presented = context
        .get_image_data(0.0, 0.0, 128.0, 128.0)
        .unwrap()
        .data()
        .0;
    for point in fixture::probes(case) {
        fixture::assert_rgba(
            pixel(&presented, point[0], point[1]),
            fixture::expected(case, point),
        );
    }
    for pattern in [1, 2] {
        data.paint(pattern);
        drop(constructed);
        constructed = data.constructed();
        renderer
            .upload_terrain_atlas(&constructed, &data.art)
            .unwrap();
        let pixels = draw_world(
            &mut renderer,
            &fixture::boundary_faces(&data.art),
            fixture::BOUNDARY,
        );
        for (actual, point) in pixels.into_iter().zip(fixture::BOUNDARY) {
            fixture::assert_rgba(
                actual,
                if pattern == 1 {
                    fixture::boundary_expected(point)
                } else {
                    [41, 74, 36, 255]
                },
            );
        }
    }
}
