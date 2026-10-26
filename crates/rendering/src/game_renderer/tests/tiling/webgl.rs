use super::*;
use crate::surface_mesh::directory::fixture;

#[wasm_bindgen_test]
fn webgl_shallow_water_fades_into_sand_at_two_zooms() {
    use crate::surface_mesh::mix::fixture as shore;
    let (canvas, mut renderer) = target(&shore::atlas());
    for zoom in [0.2, 0.4] {
        let mut packets = shore::shore_scene(zoom)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        shore::assert_shore(shore::SHORE_PROBES.map(|[x, y]| pixel(&canvas, x, y)));
        shore::assert_soft(0, &|x, y| pixel(&canvas, x, y));
    }
}

#[wasm_bindgen_test]
fn native_cliff_wall_uses_opaque_rock_above_the_source_silhouette_notch() {
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for y in 0..156 {
        for x in 0..294 {
            if x < 185 || y < 100 {
                let at = crate::GAME_ATLAS_PAGE_BYTES * 2 + ((y + 32) * 2048 + x + 32) * 4;
                pixels[at..at + 4].copy_from_slice(&[85, 90, 95, 255]);
            }
        }
    }
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [32.0 / 2048.0, 32.0 / 2048.0, 294.0 / 2048.0, 156.0 / 2048.0],
        },
        size: [294.0, 156.0],
        anchor: [153.0, 80.0],
    };
    let art = GameArt {
        actors: Vec::new(),
        buildings: std::array::from_fn(|_| Vec::new()),
        village: Default::default(),
        fishing: Default::default(),
        compositions: Vec::new(),
        foundation: Default::default(),
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: crate::SceneryArt {
            cliff_faces: vec![frame; 25],
            rocks: Vec::new(),
            ..Default::default()
        },
    };
    let mut faces = [
        triangle(
            [[20.0, 20.0], [108.0, 20.0], [108.0, 108.0]],
            [0.0; 3],
            [1.0; 3],
        ),
        triangle(
            [[20.0, 20.0], [108.0, 108.0], [20.0, 108.0]],
            [0.0; 3],
            [1.0; 3],
        ),
    ];
    for (index, face) in faces.iter_mut().enumerate() {
        face.skirt = true;
        face.pickable = false;
        face.material = 4;
        face.tint = 13;
        face.texture_mode = 4 + index as u8;
        face.order = 2 + index as u8;
    }
    apply_terrain_textures(&mut faces, &art);
    let (canvas, mut renderer) = target(&pixels);
    render(&mut renderer, &mut faces.map(|face| instance(&face)));
    for y in 22..106 {
        for x in 22..106 {
            assert_pixel(&canvas, x, y, [85, 90, 95, 255]);
        }
    }
}

#[wasm_bindgen_test]
fn webgl_snow_diamond_edges_have_no_discarded_pinholes() {
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for y in 0_i32..49 {
        for x in 0_i32..97 {
            if (x - 48).abs() + 2 * (y - 24).abs() <= 48 {
                let offset = (y as usize * 2048 + x as usize) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[210, 220, 230, 255]);
            }
        }
    }
    let (canvas, mut renderer) = target(&pixels);
    let mut face = triangle(
        [[64.0, 12.0], [116.0, 64.0], [64.0, 116.0]],
        [0.0; 3],
        [1.0; 3],
    );
    face.material = 7;
    face.tint = 6;
    face.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: [0.0, 0.0, 97.0 / 2048.0, 49.0 / 2048.0],
    });
    render(&mut renderer, &mut [instance(&face)]);
    let mut checked = 0;
    for y in 13..116 {
        for x in 64..116 {
            let p = ScreenPoint {
                x: f64::from(x) + 0.5,
                y: f64::from(y) + 0.5,
            };
            if crate::surface_mesh::barycentric(face.points, p)
                .is_some_and(|w| w.iter().all(|v| *v > 0.001))
            {
                assert!(pixel(&canvas, x, y)[0] >= 200, "snow hole at {x},{y}");
                checked += 1;
            }
        }
    }
    assert!(checked > 2000);
}

#[wasm_bindgen_test]
fn webgl_terrain_texels_are_world_anchored_across_zoom_and_mesh_lod() {
    let (canvas, mut renderer) = target(&fixture::atlas());
    let mut dirt = Vec::new();
    for case in 0..fixture::CASES {
        let mut packets = fixture::scene(case)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        for (probe, [x, y]) in fixture::PROBES.into_iter().enumerate() {
            let actual = pixel(&canvas, x, y);
            fixture::assert_probe(case, probe, actual, 1);
            if matches!(case, 2 | 3) {
                dirt.push(actual);
            }
        }
    }
    let (fine, coarse) = dirt.split_at(dirt.len() / 2);
    assert_eq!(fine, coarse);
}

#[wasm_bindgen_test]
fn webgl_blends_every_ground_material_pair_across_a_noisy_border() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, mut renderer) = target(&mix::atlas());
    for case in 0..mix::CASES {
        let mut packets = mix::scene(case).iter().map(instance).collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        for (probe, [x, y]) in mix::PROBES.into_iter().enumerate() {
            mix::assert_probe(case, probe, pixel(&canvas, x, y), 2);
        }
    }
}

#[wasm_bindgen_test]
fn webgl_four_material_junction_has_matching_pixels_on_every_shared_edge() {
    use crate::surface_mesh::mix::fixture::{self as mix, junction};
    let (canvas, mut renderer) = target(&mix::atlas());
    for pair in junction::edge_pairs() {
        let pixels = pair.map(|face| {
            render(&mut renderer, &mut [instance(&face)]);
            pixel(&canvas, 64, 64)
        });
        junction::assert_edge(&pair, pixels);
    }
}

#[wasm_bindgen_test]
fn webgl_snow_is_white_ice_stays_blue_and_their_ground_borders_are_soft() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, mut renderer) = target(&mix::snow_atlas());
    for case in [2, 3, 7, 8] {
        let mut packets = mix::snow_scene(case)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        for (probe, [x, y]) in mix::PROBES.into_iter().enumerate() {
            mix::assert_snow_probe(case, probe, pixel(&canvas, x, y));
        }
    }
}

#[wasm_bindgen_test]
fn webgl_one_level_sand_water_bank_covers_the_review_probe() {
    use crate::surface_mesh::mix::fixture::{self as shore, gap};
    let (canvas, mut renderer) = target(&shore::atlas());
    renderer.resize(256, 128).unwrap();
    for halo in [false, true] {
        let mut packets = gap::scene(halo)
            .iter()
            .map(|face| surface_instance(face, [256.0, 128.0], 0.0))
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        let [x, y] = gap::PROBE;
        gap::assert_pixel(pixel(&canvas, x, y), halo);
    }
}

#[wasm_bindgen_test]
fn webgl_building_wear_darkens_native_textured_terrain() {
    use crate::game_renderer::wear_fixture as wear;
    let mut probes = Vec::new();
    for building in [false, true] {
        let (canvas, mut renderer) = target(&fixture::atlas());
        let mut packets = wear::faces(building)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        probes.push(fixture::PROBES.map(|[x, y]| pixel(&canvas, x, y)));
    }
    wear::assert_worn(&probes[0], &probes[1]);
}

#[wasm_bindgen_test]
fn webgl_slope_shading_is_directional_and_zoom_consistent() {
    use crate::game_renderer::relief_fixture as relief;
    let mut ratios = Vec::new();
    for far in [false, true] {
        let mut probes = Vec::new();
        for rise in [0, 1, -1] {
            let (canvas, mut renderer) = target(&relief::atlas());
            let mut packets = relief::faces(far, rise)
                .iter()
                .map(instance)
                .collect::<Vec<_>>();
            render(&mut renderer, &mut packets);
            probes.push(relief::PROBES.map(|[x, y]| pixel(&canvas, x, y)));
        }
        ratios.push(relief::assert_directional(
            far, &probes[0], &probes[1], &probes[2],
        ));
    }
    relief::assert_zoom_agreement(ratios[0], ratios[1]);
}

#[wasm_bindgen_test]
fn webgl_slope_shade_is_continuous_across_shared_edges() {
    use crate::game_renderer::relief_fixture as relief;
    for (far, textured) in [(false, true), (true, true), (false, false), (true, false)] {
        let (canvas, mut renderer) = target(&relief::atlas());
        let (faces, probes, gap) = relief::seam(far, textured);
        let mut packets = faces.iter().map(instance).collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        relief::assert_continuous(far, gap, &probes.map(|[x, y]| pixel(&canvas, x, y)));
    }
}

#[wasm_bindgen_test]
fn webgl_slope_beside_missing_terrain_still_renders_brighter_than_flat() {
    use crate::game_renderer::relief_fixture as relief;
    let (canvas, mut renderer) = target(&relief::atlas());
    let (faces, probes) = relief::clipped_slope();
    let mut packets = faces.iter().map(instance).collect::<Vec<_>>();
    render(&mut renderer, &mut packets);
    relief::assert_clipped_lit(&probes.map(|[x, y]| pixel(&canvas, x, y)));
}

#[wasm_bindgen_test]
async fn webgl_pixels_survive_repeated_renderer_recreation() -> Result<(), JsValue> {
    let pixels = atlas(&[[85, 90, 95, 255]]);
    for iteration in 0..96 {
        let (canvas, mut renderer) = target(&pixels);
        let gl = webgl_test_context(&canvas).unwrap();
        assert!(
            !context_is_lost(&gl),
            "renderer recreation {iteration}: live context"
        );
        render(&mut renderer, &mut [sprite(0.0, 1.0)]);
        let actual = pixel(&canvas, 64, 64);
        assert!(
            actual
                .into_iter()
                .zip([85, 90, 95, 255])
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "renderer recreation {iteration}: {actual:?}"
        );
        drop(renderer);
        // Keep expected red diagnostics bounded; the lifecycle oracle is exact.
        if !context_is_lost(&gl) {
            return Err(JsValue::from_str(&format!(
                "renderer recreation {iteration}: Drop must retire the owned context"
            )));
        }
        // Keep render/readback synchronous to observe this exact packet,
        // then let WebDriver scrape progress between full-atlas recreations.
        browser_turn().await;
    }
    Ok(())
}
