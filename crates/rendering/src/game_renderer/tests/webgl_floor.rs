use super::*;

#[wasm_bindgen_test]
fn webgl_restored_native_forest_soil_endpoints_gradient_seam_and_dirt_primary() {
    use crate::surface_mesh::floor as fixture;
    let (canvas, mut renderer) = target(&fixture::forest_atlas());
    for material in [0, 2, 6] {
        for (floor, gradient) in [(0, false), (650, false), (1000, false), (650, true)] {
            let mut faces = fixture::forest_faces(material, floor, gradient);
            for _ in 0..2 {
                let mut packets = faces.map(|face| instance(&face));
                render(&mut renderer, &mut packets);
                for [x, y] in fixture::FOREST_PROBES {
                    assert_pixel(
                        &canvas,
                        x,
                        y,
                        fixture::forest_expected(material, floor, gradient, x),
                    );
                }
                faces.reverse();
            }
        }
    }
}

#[wasm_bindgen_test]
fn webgl_shared_floor_gradient_seam_and_manual_packet_fallback() {
    use crate::surface_mesh::floor as fixture;
    let (canvas, mut renderer) = target(&fixture::atlas());
    for interpolated in [true, false] {
        let mut faces = fixture::faces(interpolated);
        for _ in 0..2 {
            let mut packets = faces.map(|face| instance(&face));
            render(&mut renderer, &mut packets);
            for [x, y] in fixture::PROBES {
                assert_pixel(&canvas, x, y, fixture::expected(x, interpolated));
            }
            faces.reverse();
        }
    }
}

#[wasm_bindgen_test]
fn webgl_water_depth_classes_draw_their_sheets_and_blend_at_borders() {
    use crate::surface_mesh::water_fixture as fixture;
    let (canvas, mut renderer) = target(&fixture::atlas());
    for (left, right) in fixture::pairs() {
        let faces = fixture::faces(left, right);
        let mut packets = faces.map(|face| instance(&face));
        render(&mut renderer, &mut packets);
        for [x, y] in fixture::PROBES {
            assert_pixel(&canvas, x, y, fixture::expected(left, right, x));
        }
    }
}

#[wasm_bindgen_test]
fn webgl_v2_floor_pixels_match_canvas_kernel_and_preserve_three_page_abi() {
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for (page, texel) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        pixels[start..start + 4].copy_from_slice(texel);
    }
    let (canvas, mut renderer) = target(&pixels);
    let address = |page| crate::AtlasAddress {
        page,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    let mut face = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [0.0; 3],
        [0.0; 3],
    );
    face.texture_uv = Some(address(0));
    face.texture_blend = Some([address(1), address(2)]);
    assert_eq!(instance(&face).pages[3], 0);
    let frame = |page| GameFrame {
        atlas: address(page),
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let mut art = GameArt {
        buildings: Default::default(),
        village: Default::default(),
        town_center: Default::default(),
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame(0)],
        terrain: std::array::from_fn(|_| vec![frame(0)]),
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
    };
    art.terrain[2] = vec![frame(1)];
    art.terrain[6] = vec![frame(2)];
    for palette in 0..6 {
        face.appearance =
            crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                floor_strength: 650,
                canopy_strength: 650,
                palette,
                exposure: 1,
                height_band: 2,
            }));
        face.tint = 1;
        let packet = instance(&face);
        assert_eq!(packet.pages[..3], [0, 1, 2]);
        let expected = crate::surface_mesh::landscape::texel(
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
            crate::surface_mesh::landscape::floor_weights(packet.pages[3]),
            1,
            packet.pages[3],
        );
        render(&mut renderer, &mut [packet]);
        assert_pixel(&canvas, 48, 48, expected);
        assert_pixel(&canvas, 32, 32, expected);
        assert_ne!(expected, [82, 86, 86, 255]);
        for floor_strength in [0, 650, 1000] {
            let mut dirt = face;
            dirt.material = 2;
            dirt.appearance =
                crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                    floor_strength,
                    canopy_strength: 650,
                    palette,
                    exposure: 1,
                    height_band: 2,
                }));
            apply_terrain_textures(std::slice::from_mut(&mut dirt), &art);
            let packet = instance(&dirt);
            assert_eq!(dirt.texture_uv, Some(address(1)));
            assert_eq!(dirt.texture_blend, Some([address(2), address(1)]));
            assert_eq!(packet.pages[..3], [1, 2, 1]);
            let expected = crate::surface_mesh::landscape::texel(
                [[0, 255, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]],
                crate::surface_mesh::landscape::floor_weights(packet.pages[3]),
                1,
                packet.pages[3],
            );
            render(&mut renderer, &mut [packet]);
            assert_pixel(&canvas, 48, 48, expected);
            assert_pixel(&canvas, 32, 32, expected);
        }
    }
}
