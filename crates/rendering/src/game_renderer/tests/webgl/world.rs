//! Actual linked WebGL2 driver/presented-pixel coverage of world provenance.
use super::*;
use crate::surface_mesh::{ProjectedSurfaceTriangle, world::fixtures as fixture};

#[path = "world/restore.rs"]
mod restore;

fn draw_world(
    canvas: &HtmlCanvasElement,
    renderer: &mut WebGlRenderer,
    faces: &[ProjectedSurfaceTriangle],
    points: [[u32; 2]; 5],
) -> [[u8; 4]; 5] {
    let mut packets = faces
        .iter()
        .map(|face| {
            let packet = instance(face);
            assert_eq!(
                packet.pages[3] & (1 << 30) != 0,
                face.world_texture().is_some()
            );
            packet
        })
        .collect::<Vec<_>>();
    render(renderer, &mut packets);
    points.map(|[x, y]| pixel(canvas, x, y))
}

#[wasm_bindgen_test]
fn webgl_world_native_phase_lod_zoom_alpha_and_replacement_pixels() {
    let mut data = fixture::Fixture::new();
    let mut constructed = data.constructed();
    // No full raw upload before typed admission; legacy target() is unchanged.
    let canvas = web_sys::window()
        .expect("browser window")
        .document()
        .expect("browser document")
        .create_element("canvas")
        .expect("create canvas")
        .dyn_into::<HtmlCanvasElement>()
        .expect("canvas element");
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer =
        WebGlRenderer::new(&canvas).expect("actual WebGL2 context and linked shaders");
    assert_eq!(renderer.world_atlas, None);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    for case in fixture::cases() {
        let points = fixture::probes(case);
        let mut faces = fixture::faces(&data.art, case, false);
        let expected = points.map(|point| fixture::expected(case, point));
        let coarse = draw_world(&canvas, &mut renderer, &faces, points);
        let fine = draw_world(
            &canvas,
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
        let reversed = draw_world(&canvas, &mut renderer, &faces, points);
        let mut other_zoom = case;
        other_zoom.zoom = !case.zoom;
        let zoomed = draw_world(
            &canvas,
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
    renderer.upload(&data.atlas).unwrap();
    assert_eq!(renderer.world_atlas, None);
    let raw = draw_world(&canvas, &mut renderer, &cached, fixture::probes(case));
    let old = draw_world(&canvas, &mut renderer, &legacy, fixture::probes(case));
    assert_eq!(raw, old);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    data.reject_invalid_art(&constructed, |atlas, art| {
        renderer.upload_terrain_atlas(atlas, art)
    });
    assert_eq!(renderer.world_atlas, Some(old_key));
    assert!(renderer.upload(&[]).is_err());
    assert_eq!(renderer.world_atlas, None);
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
        renderer.upload(&data.atlas).unwrap();
        let stale = draw_world(&canvas, &mut renderer, &cached, fixture::probes(case));
        let old = draw_world(&canvas, &mut renderer, &legacy, fixture::probes(case));
        for i in 0..5 {
            fixture::assert_rgba(stale[i], old[i]);
        }
        // Test-only fault injection checks the shader, not only CPU downgrade.
        renderer.world_atlas = Some(old_key);
        let guarded = draw_world(&canvas, &mut renderer, &cached, fixture::probes(case));
        assert_eq!(guarded, old);
    }
    drop(constructed);
    constructed = data.constructed();
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    let fresh = draw_world(
        &canvas,
        &mut renderer,
        &fixture::faces(&data.art, case, false),
        fixture::probes(case),
    );
    for (actual, point) in fresh.into_iter().zip(fixture::probes(case)) {
        fixture::assert_rgba(actual, fixture::expected(case, point));
    }
    for pattern in [1, 2] {
        data.paint(pattern);
        drop(constructed);
        constructed = data.constructed();
        renderer
            .upload_terrain_atlas(&constructed, &data.art)
            .unwrap();
        let pixels = draw_world(
            &canvas,
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
