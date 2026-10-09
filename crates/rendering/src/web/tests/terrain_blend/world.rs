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
        .read(renderer, faces.len() as u32, probes, [0.0, 0.0, 0.0, 1.0])
        .await
}

#[wasm_bindgen_test]
async fn webgpu_world_native_phase_lod_zoom_alpha_and_replacement_pixels() {
    let mut data = fixture::Fixture::new();
    let mut constructed = data.constructed();
    // Start with the diagnostic texture; upload the full atlas only once,
    // through typed admission. The legacy shared raw builder stays unchanged.
    let canvas = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer = Renderer::new(canvas)
        .await
        .expect("software WebGPU renderer");
    let diagnostic = renderer.render_sprites(&[]).unwrap();
    assert_eq!(diagnostic.atlas_pages, 1);
    assert_eq!(diagnostic.atlas_bytes, 8 * 8 * 4);
    assert_eq!(renderer.world_atlas, None);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    let mut reader = PixelReadback::<5>::new(&renderer);
    for (case_index, case) in fixture::cases().into_iter().enumerate() {
        let points = fixture::probes(case);
        let mut faces = fixture::faces(&data.art, case, false);
        let expected = points.map(|point| fixture::expected(case, point));
        let coarse = draw(&mut renderer, &mut reader, &faces, points).await;
        if case_index == 0 {
            let mut packets = faces
                .iter()
                .map(|face| surface_instance(face, [128.0; 2], 0.0))
                .collect::<Vec<_>>();
            let original = bytemuck::cast_slice::<_, u8>(&packets).to_vec();
            let borrowed = renderer
                .render_sprites_with_clear(&packets, [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            assert_eq!(bytemuck::cast_slice::<_, u8>(&packets), original);
            let borrowed_pixels = reader
                .read(
                    &renderer,
                    packets.len() as u32,
                    points,
                    [0.0, 0.0, 0.0, 1.0],
                )
                .await;
            let owned = renderer
                .render_owned_sprites(&mut packets, [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            let owned_pixels = reader
                .read(
                    &renderer,
                    packets.len() as u32,
                    points,
                    [0.0, 0.0, 0.0, 1.0],
                )
                .await;
            assert_eq!(borrowed.visible, owned.visible);
            assert_eq!(borrowed.draw_calls, owned.draw_calls);
            assert_eq!(borrowed.gpu_buffer_bytes, owned.gpu_buffer_bytes);
            assert_eq!(
                borrowed.persistent_gpu_resources,
                owned.persistent_gpu_resources
            );
            assert_eq!(borrowed.atlas_bytes, owned.atlas_bytes);
            assert_eq!(borrowed_pixels, owned_pixels);
            assert_eq!(owned_pixels, coarse);
        }
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
    // Matching, fully valid bytes uploaded RAW must still disable lookup.
    renderer.upload_game_atlas(&data.atlas).unwrap();
    assert_eq!(renderer.world_atlas, None);
    let raw = draw(&mut renderer, &mut reader, &cached, fixture::probes(case)).await;
    let old = draw(&mut renderer, &mut reader, &legacy, fixture::probes(case)).await;
    assert_eq!(raw, old);
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    data.reject_invalid_art(&constructed, |atlas, art| {
        renderer.upload_terrain_atlas(atlas, art)
    });
    assert_eq!(renderer.world_atlas, Some(old_key));
    assert!(renderer.upload_game_atlas(&[]).is_err());
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
        renderer.upload_game_atlas(&data.atlas).unwrap();
        let stale = draw(&mut renderer, &mut reader, &cached, fixture::probes(case)).await;
        let old = draw(&mut renderer, &mut reader, &legacy, fixture::probes(case)).await;
        for i in 0..5 {
            fixture::assert_rgba(stale[i], old[i]);
        }
        // Test-only fault injection bypasses the private CPU admission gate.
        // The vertex-stage header/key check must still restore legacy pixels.
        renderer.world_atlas = Some(old_key);
        let guarded = draw(&mut renderer, &mut reader, &cached, fixture::probes(case)).await;
        assert_eq!(guarded, old);
    }
    drop(constructed);
    constructed = data.constructed();
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
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
        drop(constructed);
        constructed = data.constructed();
        renderer
            .upload_terrain_atlas(&constructed, &data.art)
            .unwrap();
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
