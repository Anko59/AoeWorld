use super::*;

#[wasm_bindgen_test]
fn canvas_shared_floor_gradient_seam_and_old_packet_fallback() {
    use crate::surface_mesh::floor as fixture;
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = fixture::atlas();
    for interpolated in [true, false] {
        let mut faces = fixture::faces(interpolated);
        for _ in 0..2 {
            canvas_depth::render_canvas_world(
                &canvas,
                &context,
                &atlas,
                &mut presentation,
                &faces.map(WorldLayer::Surface),
                test_camera(),
                false,
            )
            .unwrap();
            for [x, y] in fixture::PROBES {
                assert_eq!(
                    pixel(&presentation.color_buffer, x, y),
                    fixture::expected(x, interpolated)
                );
            }
            faces.reverse();
        }
    }
}

fn blended_triangle() -> ProjectedSurfaceTriangle {
    let mut triangle = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [0.0; 3],
        [0.0; 3],
    );
    let rect = |x: f32| [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0];
    triangle.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: rect(0.0),
    });
    triangle.texture_blend = Some([
        crate::AtlasAddress {
            page: 1,
            uv: rect(0.0),
        },
        crate::AtlasAddress {
            page: 2,
            uv: rect(0.0),
        },
    ]);
    triangle
}

#[wasm_bindgen_test]
fn canvas_v2_floor_pixels_are_uniform_and_distinct_from_legacy() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = blend_atlas();
    let mut face = blended_triangle();
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &[WorldLayer::Surface(face)],
        test_camera(),
        false,
    )
    .unwrap();
    let legacy = pixel(&presentation.color_buffer, 48, 48);
    for palette in 0..4 {
        face.appearance =
            crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                floor_strength: 650,
                canopy_strength: 650,
                palette,
                exposure: 1,
                height_band: 2,
            }));
        face.tint = 1;
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &[WorldLayer::Surface(face)],
            test_camera(),
            false,
        )
        .unwrap();
        let word = face.appearance;
        let expected = crate::surface_mesh::landscape::texel(
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
            crate::surface_mesh::landscape::floor_weights(word),
            1,
            word,
        );
        assert_eq!(pixel(&presentation.color_buffer, 48, 48), expected);
        assert_eq!(pixel(&presentation.color_buffer, 32, 32), expected);
        assert_ne!(expected, legacy);
    }
}

#[wasm_bindgen_test]
fn canvas_landscape_dirt_pixels_use_authoritative_primary_after_texture_assignment() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = blend_atlas();
    let frame = |page| GameFrame {
        atlas: crate::AtlasAddress {
            page,
            uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let art = GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame(0)],
        terrain: std::array::from_fn(|material| {
            vec![frame(match material {
                2 => 1,
                6 => 2,
                _ => 0,
            })]
        }),
        terrain_topology: [None; 7],
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
    };
    for palette in 0..6 {
        for floor_strength in [0, 650] {
            let mut face = blended_triangle();
            face.material = 2;
            face.tint = 1;
            face.appearance =
                crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                    floor_strength,
                    canopy_strength: 650,
                    palette,
                    exposure: 1,
                    height_band: 2,
                }));
            apply_terrain_textures(std::slice::from_mut(&mut face), &art);
            assert_eq!(face.texture_uv, Some(frame(1).atlas));
            assert_eq!(face.texture_blend, Some([frame(2).atlas, frame(1).atlas]));
            let expected = crate::surface_mesh::landscape::texel(
                [[0, 255, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]],
                crate::surface_mesh::landscape::floor_weights(face.appearance),
                1,
                face.appearance,
            );
            canvas_depth::render_canvas_world(
                &canvas,
                &context,
                &atlas,
                &mut presentation,
                &[WorldLayer::Surface(face)],
                test_camera(),
                false,
            )
            .unwrap();
            assert_eq!(pixel(&presentation.color_buffer, 48, 48), expected);
            assert_eq!(pixel(&presentation.color_buffer, 32, 32), expected);
            assert_eq!(
                context
                    .get_image_data(48.0, 48.0, 1.0, 1.0)
                    .unwrap()
                    .data()
                    .0,
                expected
            );
        }
    }
}

fn blend_atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for (page, texel) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        atlas[start..start + 4].copy_from_slice(texel);
    }
    atlas
}

#[wasm_bindgen_test]
fn canvas_procedural_material_pixels_match_shared_kernel() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = blend_atlas();
    for tint in (5..=10).chain([12]).chain(21..=26) {
        let mut face = blended_triangle();
        face.texture_blend = None;
        face.tint = tint;
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &[WorldLayer::Surface(face)],
            test_camera(),
            false,
        )
        .unwrap();
        assert_eq!(
            pixel(&presentation.color_buffer, 48, 48),
            crate::surface_mesh::procedural_tint([255, 0, 0, 255], tint)
        );
    }
}

#[wasm_bindgen_test]
fn canvas_upload_keeps_one_flat_owner_and_creates_only_requested_legacy_page() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut renderer = GameRenderer::Canvas {
        canvas: canvas.clone(),
        context,
        atlas: [None, None, None],
        source_atlas: Vec::new(),
        presentation: CanvasPresentation::new(128, 128),
    };
    renderer.upload_game_atlas(&blend_atlas()).unwrap();
    let GameRenderer::Canvas {
        atlas,
        source_atlas,
        ..
    } = &mut renderer
    else {
        unreachable!()
    };
    assert_eq!(source_atlas.len(), crate::GAME_ATLAS_BYTES);
    assert!(atlas.iter().all(Option::is_none));
    let page = legacy_page(&canvas, atlas, source_atlas, 1).unwrap();
    assert_eq!(
        super::context(page)
            .unwrap()
            .get_image_data(0.0, 0.0, 1.0, 1.0)
            .unwrap()
            .data()
            .0,
        [0, 255, 0, 255]
    );
    assert!(atlas[0].is_none() && atlas[1].is_some() && atlas[2].is_none());
    assert!(legacy_page(&canvas, atlas, source_atlas, 3).is_err());
}

fn assert_blended(pixel: &[u8]) {
    for (actual, expected) in pixel.iter().zip([82_u8, 86, 86, 255]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "unexpected blend pixel: {pixel:?}"
        );
    }
}

#[wasm_bindgen_test]
fn canvas_crossfades_three_native_materials_with_shared_barycentric_weights() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &blend_atlas(),
        &mut presentation,
        &[WorldLayer::Surface(blended_triangle())],
        test_camera(),
        false,
    )
    .expect("Canvas splat rendering");
    assert_blended(&pixel(&presentation.color_buffer, 48, 48));
    assert_eq!(presentation.depth_buffer[48 * 128 + 48], 0.0);
}

#[wasm_bindgen_test]
fn canvas_multiplies_shadow_sprite_tint_and_alpha_like_webgpu() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let sprite = crate::web::Sprite {
        position: [0.0; 2],
        radius: [0.25; 2],
        color: [0.0, 0.0, 0.0, 0.5],
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        depths: [1.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [2, 0, 0, 0],
    };
    let frame = crate::GameFrame {
        atlas: crate::AtlasAddress {
            page: sprite.pages[0],
            uv: sprite.uv,
        },
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let layer = WorldLayer::Sprite(sprite, frame, 1.0, 0);
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    let start = 2 * crate::GAME_ATLAS_PAGE_BYTES;
    atlas[start..start + 4].copy_from_slice(&[255; 4]);
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &[layer],
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&presentation.color_buffer, 64, 64), [20, 37, 18, 255]);
    assert_eq!(presentation.depth_buffer[64 * 128 + 64], 1.0);
}
