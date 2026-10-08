//! One actual WebGPU case, bounded matrices; never a replacement/filter for the full gate.
use super::*;
use crate::surface_mesh::{ProjectedSurfaceTriangle, world::fixtures as fixture};

async fn draw(
    renderer: &mut Renderer,
    reader: &mut PixelReadback<5>,
    faces: &[ProjectedSurfaceTriangle],
    probes: [[u32; 2]; 5],
) -> [[u8; 4]; 5] {
    assert_eq!(std::mem::size_of::<crate::Sprite>(), 112);
    for face in faces {
        let packet = surface_instance(face, [128.0; 2], 0.0);
        assert_eq!(
            packet.pages[3] & (1 << 30) != 0,
            face.world_texture().is_some()
        );
    }
    renderer
        .render_world_layers(faces, &[], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    reader
        .read(renderer, faces.len() as u32, probes, wgpu::Color::BLACK)
        .await
}

#[wasm_bindgen_test]
async fn webgpu_world_native_phase_lod_zoom_alpha_and_replacement_pixels() {
    let mut data = fixture::Fixture::new();
    let mut renderer = surface_renderer_with_atlas(&data.atlas).await;
    let mut reader = PixelReadback::<5>::new(&renderer);
    for case in fixture::cases() {
        let points = fixture::probes(case);
        let mut faces = fixture::faces(&data.art, case, false);
        let expected = points.map(|point| fixture::expected(case, point));
        let coarse = draw(&mut renderer, &mut reader, &faces, points).await;
        let fine = draw(
            &mut renderer,
            &mut reader,
            &fixture::faces(&data.art, case, true),
            points,
        )
        .await;
        for i in 0..5 {
            fixture::assert_rgba(coarse[i], expected[i]);
            fixture::assert_rgba(fine[i], expected[i]);
            fixture::assert_rgba(fine[i], coarse[i]);
        }
        faces.reverse();
        let reversed = draw(&mut renderer, &mut reader, &faces, points).await;
        let mut other_zoom = case;
        other_zoom.zoom = !case.zoom;
        let zoomed = draw(
            &mut renderer,
            &mut reader,
            &fixture::faces(&data.art, other_zoom, false),
            fixture::probes(other_zoom),
        )
        .await;
        for i in 0..5 {
            fixture::assert_rgba(reversed[i], coarse[i]);
            fixture::assert_rgba(zoomed[i], coarse[i]);
        }
    }
    let case = fixture::cases()[4];
    let cached = fixture::faces(&data.art, case, false);
    let legacy = fixture::legacy(&cached);
    let old_key = data.art.terrain_world.unwrap();
    for replacement in 0..3 {
        if replacement < 2 {
            data.corrupt(replacement == 1);
        } else {
            data.layout(true);
            assert_ne!(data.art.terrain_world.unwrap(), old_key);
        }
        renderer.upload_game_atlas(&data.atlas).unwrap();
        let stale = draw(&mut renderer, &mut reader, &cached, fixture::probes(case)).await;
        let old = draw(&mut renderer, &mut reader, &legacy, fixture::probes(case)).await;
        for i in 0..5 {
            fixture::assert_rgba(stale[i], old[i]);
        }
    }
    let fresh = draw(
        &mut renderer,
        &mut reader,
        &fixture::faces(&data.art, case, false),
        fixture::probes(case),
    )
    .await;
    for (pixel, point) in fresh.into_iter().zip(fixture::probes(case)) {
        fixture::assert_rgba(pixel, fixture::expected(case, point));
    }
    for pattern in [1, 2] {
        data.paint(pattern);
        renderer.upload_game_atlas(&data.atlas).unwrap();
        let pixels = draw(
            &mut renderer,
            &mut reader,
            &fixture::boundary_faces(&data.art),
            fixture::BOUNDARY,
        )
        .await;
        for (pixel, point) in pixels.into_iter().zip(fixture::BOUNDARY) {
            fixture::assert_rgba(
                pixel,
                if pattern == 1 {
                    fixture::boundary_expected(point)
                } else {
                    [0, 0, 0, 255]
                },
            );
        }
    }
    drop(reader);
    renderer.device.destroy();
}
