//! Mesh contract of ground mixing: which faces mix, with which layers and shares.
use super::*;
use std::collections::BTreeMap;

/// Grass, dry grass, dirt, sand, rock, snow, ice, mud, then the variety
/// sheets: grass and dirt mix, worn grass, lush grass and gravel.
const LAND: [u8; 12] = [0, 1, 2, 3, 4, 7, 8, 9, 13, 14, 15, 16];

fn sample(x: i32, y: i32, material: u8) -> SceneTerrain {
    SceneTerrain {
        position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
        material,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: Some(crate::SceneTerrainAppearance {
            floor_strength: 0,
            canopy_strength: 0,
            palette: 5,
            exposure: 0,
            height_band: 0,
        }),
    }
}

/// West of x = 0 is `west`, the rest `east`.
fn halves(west: u8, east: u8) -> Vec<SceneTerrain> {
    (-6..6)
        .flat_map(|y| (-6..6).map(move |x| sample(x, y, if x < 0 { west } else { east })))
        .collect()
}

fn mesh(terrain: &[SceneTerrain], zoom: f64) -> Vec<ProjectedSurfaceTriangle> {
    let camera = super::appearance_tests::camera([0.0; 2], zoom, [1024.0, 768.0]);
    projected_surface_triangles(terrain, camera)
}

fn assert_water_side_layers_unchanged(material: u8, kind: u8) {
    // The wet tile touches both grass and sand, so ground support contains
    // multiple materials even though the water face must retain its own path.
    let mut terrain = vec![sample(0, 0, material), sample(1, 0, 0), sample(0, 1, 3)];
    terrain[0].surface.water = kind;
    let art = crate::surface_mesh::water_fixture::art(true);
    let mut faces = mesh(&terrain, 1.0);
    // Metadata-free water takes the existing water-only vertex/layer path.
    terrain[0].appearance = None;
    let mut reference = mesh(&terrain, 1.0);
    apply_terrain_textures(&mut faces, &art);
    apply_terrain_textures(&mut reference, &art);
    let wet = |face: &&ProjectedSurfaceTriangle| !face.skirt && face.tile == [0, 0];
    let water_faces = faces.iter().filter(wet).collect::<Vec<_>>();
    let reference_faces = reference.iter().filter(wet).collect::<Vec<_>>();
    assert_eq!(water_faces.len(), 2);
    for (face, expected) in water_faces.iter().zip(reference_faces) {
        assert_eq!(face.mix, [0; 6], "material {material}, kind {kind}");
        assert_eq!(face.texture_materials, expected.texture_materials);
        assert_eq!(face.texture_uv, expected.texture_uv);
        assert_eq!(face.texture_blend, expected.texture_blend);
        assert_eq!(face.texture_mode, expected.texture_mode);
        assert_eq!(face.tint, expected.tint);
        assert!(face.texture_uv.is_some());
        assert_eq!(face.texture_materials, Some([material; 3]));
    }
    let land = faces
        .iter()
        .filter(|face| !face.skirt && face.tile != [0, 0]);
    let mut land_count = 0;
    for face in land {
        land_count += 1;
        assert!(
            face.texture_materials
                .unwrap()
                .into_iter()
                .all(|layer| { !crate::surface_mesh::appearance::water_material(layer) }),
            "water/Ford layer on land beside material {material}, kind {kind}"
        );
    }
    assert_eq!(land_count, 4);
}

#[wasm_bindgen_test]
fn lake_ocean_and_river_depth_faces_keep_water_layers_beside_mixed_land() {
    for kind in 1..=4 {
        for material in [5, 10, 11] {
            assert_water_side_layers_unchanged(material, kind);
        }
    }
}

#[wasm_bindgen_test]
fn ford_faces_keep_water_layers_and_never_supply_ground_layers() {
    assert_water_side_layers_unchanged(12, 0);
}

/// Overlay share by displayed vertex; a vertex must agree on every face.
fn shares(faces: &[ProjectedSurfaceTriangle]) -> BTreeMap<[i32; 2], u8> {
    let mut shares = BTreeMap::new();
    for face in faces.iter().filter(|face| face.mix != [0; 6]) {
        for (corner, point) in face.points.iter().enumerate() {
            let key = [point.world[0] as i32, point.world[1] as i32];
            let share = face.mix[corner];
            assert_eq!(*shares.entry(key).or_insert(share), share, "vertex {key:?}");
        }
    }
    shares
}

#[wasm_bindgen_test]
fn every_pair_of_land_materials_mixes_across_its_border() {
    let frame = |material: u8| GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [f32::from(material) / 16.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    };
    let mut art = test_art(frame(0));
    for group in 0..11 {
        art.terrain[group] = vec![frame(group as u8)];
    }
    for west in LAND {
        for east in LAND {
            let mut faces = mesh(&halves(west, east), 1.0);
            let mixed = faces.iter().filter(|face| face.mix != [0; 6]).count();
            if west == east {
                assert_eq!(mixed, 0, "uniform {west} must not mix");
                continue;
            }
            let (base, over) = if mix::rank(west) < mix::rank(east) {
                (west, east)
            } else {
                (east, west)
            };
            let shares = shares(&faces);
            for y in -3..=3 {
                // The border column is half overlay; one tile either side is pure.
                assert_eq!(shares.get(&[0, y]), Some(&128), "{west}/{east}");
                let pure = if over == east { [0, 255] } else { [255, 0] };
                assert_eq!(shares.get(&[-1, y]), Some(&pure[0]), "{west}/{east}");
                assert_eq!(shares.get(&[1, y]), Some(&pure[1]), "{west}/{east}");
            }
            apply_terrain_textures(&mut faces, &art);
            for face in &faces {
                let touches_border = face.points.iter().any(|point| point.world[0] == 0.0);
                assert_eq!(face.mix != [0; 6], touches_border, "{west}/{east}");
                if !touches_border {
                    continue;
                }
                // Both sides of the border draw the same ordered layers.
                let layers = face.texture_materials.expect("mixed layers");
                assert_eq!((layers[0], layers[2]), (base, over), "{west}/{east}");
                assert_eq!(face.texture_mode, directory::TILED);
                assert!(
                    face.tint <= 3,
                    "per-layer procedural ground, not a face tint"
                );
                assert!(face.texture_uv.is_some() && face.texture_blend.is_some());
            }
        }
    }
}

#[wasm_bindgen_test]
fn mixing_is_independent_of_input_order_mesh_lod_and_unmixed_scenes() {
    let mut terrain = (-40_i32..40)
        .flat_map(|y| {
            (-40_i32..40).map(move |x| {
                let material =
                    LAND[(x.div_euclid(5) * 3 + y.div_euclid(7)).rem_euclid(12) as usize];
                sample(x, y, material)
            })
        })
        .collect::<Vec<_>>();
    for zoom in [1.0, 0.12] {
        let forward = mesh(&terrain, zoom);
        terrain.reverse();
        let mut backward = mesh(&terrain, zoom);
        assert!(forward.iter().any(|face| face.mix != [0; 6]));
        // Faces sort deterministically; mixing must not depend on sample order.
        let key = |face: &ProjectedSurfaceTriangle| (face.texture_tile, face.order);
        backward.sort_by_key(key);
        let mut sorted = forward.clone();
        sorted.sort_by_key(key);
        assert_eq!(sorted.len(), backward.len());
        for (a, b) in sorted.iter().zip(&backward) {
            assert_eq!((a.mix, a.texture_materials), (b.mix, b.texture_materials));
            assert_eq!(a.floor_strengths, b.floor_strengths);
        }
    }
    // Metadata-free scenes and water never mix: their existing paths are intact.
    let mut plain = halves(0, 3);
    for sample in &mut plain {
        sample.appearance = None;
    }
    assert!(mesh(&plain, 1.0).iter().all(|face| face.mix == [0; 6]));
    let mut coast = halves(0, 5);
    for sample in coast.iter_mut().filter(|sample| sample.material == 5) {
        sample.surface.water = 2;
    }
    assert!(mesh(&coast, 1.0).iter().all(|face| face.mix == [0; 6]));
}

#[wasm_bindgen_test]
fn forest_bed_and_a_third_material_share_three_layers() {
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    };
    let mut art = test_art(frame);
    for group in 0..7 {
        art.terrain[group][0].atlas.uv[0] = group as f32 / 16.0;
    }
    // Dirt, sand and grass columns: the middle tile column sees all three.
    let strips = |forest: bool| -> Vec<SceneTerrain> {
        (-6..6)
            .flat_map(|y| {
                (-6..6).map(move |x| {
                    let mut tile = sample(x, y, [2, 3, 0][(x + 1).clamp(0, 2) as usize]);
                    if forest && x < 0 {
                        tile.appearance.as_mut().unwrap().floor_strength = 1000;
                    }
                    tile
                })
            })
            .collect()
    };
    for forest in [false, true] {
        let mut faces = mesh(&strips(forest), 1.0);
        apply_terrain_textures(&mut faces, &art);
        let middle = faces
            .iter()
            .filter(|face| !face.skirt && face.tile == [0, 0])
            .collect::<Vec<_>>();
        assert_eq!(middle.len(), 2);
        for face in middle {
            let layers = face.texture_materials.unwrap();
            assert_eq!((layers[0], layers[2]), (3, 0), "sand under grass");
            if forest {
                // Leaf litter takes the middle layer; dirt folds into the base.
                assert_eq!(layers[1], 6);
                assert_eq!(face.mix[3..], [0; 3]);
                assert!(face.floor_strengths.unwrap().iter().any(|floor| *floor > 0));
            } else {
                assert_eq!(layers[1], 2);
                assert!(face.mix[3..].iter().any(|share| *share > 0));
            }
        }
    }
}

#[wasm_bindgen_test]
fn noisy_threshold_keeps_pure_ground_pure_and_weights_normalized() {
    let mut low = f32::MAX;
    let mut high = f32::MIN;
    for y in -40..40 {
        for x in -40..40 {
            for inside in [[0.0, 0.0], [0.3, 0.8], [0.99, 0.5]] {
                let noise = mix::noise([x, y], inside);
                assert!((0.0..=1.0).contains(&noise));
                low = low.min(noise);
                high = high.max(noise);
                assert_eq!(mix::weights([0.0, 0.0, 0.0], false, noise), [1.0, 0.0, 0.0]);
                assert_eq!(mix::weights([1.0, 0.0, 0.0], false, noise), [0.0, 0.0, 1.0]);
                assert_eq!(mix::weights([0.0, 1.0, 0.0], true, noise), [0.0, 1.0, 0.0]);
                for shares in [[0.4, 0.3, 0.0], [0.5, 0.0, 0.0], [0.2, 0.6, 0.0]] {
                    let weights = mix::weights(shares, true, noise);
                    assert!(weights.iter().all(|weight| (0.0..=1.0).contains(weight)));
                    assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                }
            }
        }
    }
    assert!(
        low < 0.2 && high > 0.8,
        "noise spans its range: {low}..{high}"
    );
    // Continuous across a tile edge: the border meanders without seams.
    let step = (mix::noise([3, 5], [0.9999, 0.4]) - mix::noise([4, 5], [0.0, 0.4])).abs();
    assert!(step < 0.01, "{step}");
    // The same share from either side of a border gives complementary cover.
    assert!(mix::weights([0.5, 0.0, 0.0], false, 0.9)[2] > 0.9);
    assert!(mix::weights([0.5, 0.0, 0.0], false, 0.1)[2] < 0.1);
}

#[wasm_bindgen_test]
fn four_material_quad_uses_identical_layers_and_shares_on_its_diagonal() {
    let faces = mix::fixture::junction::scene();
    let quad = faces
        .iter()
        .filter(|face| face.tile == [0, 0])
        .collect::<Vec<_>>();
    assert_eq!(quad.len(), 2);
    assert_eq!(quad[0].texture_materials, quad[1].texture_materials);
    let palette = |face: &ProjectedSurfaceTriangle| {
        let packet = crate::web::surface_instance(face, [128.0; 2], 0.0);
        [packet.terrain_blend[0][3], packet.terrain_blend[1][3]]
    };
    assert_eq!(
        palette(quad[0]),
        palette(quad[1]),
        "ordered junction sheets"
    );
    assert_eq!(
        mix::fixture::junction::diagonal_colour(quad[0]),
        mix::fixture::junction::diagonal_colour(quad[1])
    );
    for world in [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]] {
        let corner = |face: &ProjectedSurfaceTriangle| {
            face.points
                .iter()
                .position(|point| point.world == world)
                .unwrap()
        };
        let (a, b) = (corner(quad[0]), corner(quad[1]));
        assert_eq!(
            [quad[0].mix[a], quad[0].mix[3 + a]],
            [quad[1].mix[b], quad[1].mix[3 + b]]
        );
    }
}

#[wasm_bindgen_test]
fn variety_sheets_draw_their_own_art_or_fall_back_to_the_base_sheet() {
    let frame = |group: usize| GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [group as f32 / 16.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    };
    let pack = |missing: Option<usize>| {
        let mut art = test_art(frame(0));
        for group in 0..11 {
            art.terrain[group] = vec![frame(group)];
        }
        if let Some(group) = missing {
            art.terrain[group].clear();
        }
        art
    };
    let art = pack(None);
    // Scene 13–16: grass and dirt mix, worn grass, lush grass, gravel.
    for (material, group, base) in [(13, 7, 1), (14, 8, 0), (15, 9, 0), (16, 10, 3)] {
        let own = terrain_texture_frame(&art, material, [3, -2]).expect("variety sheet");
        assert_eq!(own.atlas, frame(group).atlas);
        let partial = pack(Some(group));
        let fallback = terrain_texture_frame(&partial, material, [3, -2]).expect("base sheet");
        assert_eq!(fallback.atlas, frame(base).atlas, "material {material}");
        // A uniform stretch of a variety sheet is one plain layer; the grassy
        // ones keep the ecological palette, gravel does not.
        let terrain = (-3..3)
            .flat_map(|y| (-3..3).map(move |x| sample(x, y, material)))
            .collect::<Vec<_>>();
        let mut faces = mesh(&terrain, 1.0);
        apply_terrain_textures(&mut faces, &art);
        for face in &faces {
            assert_eq!(face.mix, [0; 6]);
            assert_eq!(face.texture_uv, Some(frame(group).atlas));
            assert!(face.texture_blend.is_none());
            assert_eq!(face.vegetated(), material != 16);
        }
    }
    assert!(mix::rank(16) < mix::rank(3) && mix::rank(2) < mix::rank(13));
    assert!(mix::rank(1) < mix::rank(14) && mix::rank(0) < mix::rank(15));
}

#[wasm_bindgen_test]
fn variety_junction_retains_all_scene_materials_above_water_codes() {
    let faces = mix::fixture::junction::variety_scene();
    let quad = faces
        .iter()
        .filter(|face| face.tile == [0, 0])
        .collect::<Vec<_>>();
    assert_eq!(quad.len(), 2);
    for face in quad {
        let packed = mix::support::packed(face).expect("four-material junction");
        let palette = mix::support::palette(packed);
        let materials = palette
            .into_iter()
            .filter(|code| *code != 15)
            .map(mix::support::material)
            .collect::<Vec<_>>();
        assert_eq!(materials, [16, 13, 14, 15]);
        assert_eq!(mix::support::palette_words(palette)[0] as u32 & 15, 6);
    }
}

#[wasm_bindgen_test]
fn snow_ramps_have_diffuse_shading_and_match_their_mixed_border() {
    for source in [
        [75, 120, 170, 255],
        [160, 220, 255, 128],
        [235, 245, 255, 255],
    ] {
        let flat = procedural_tint(source, 6);
        let ramp = procedural_tint(source, 22);
        for channel in 0..3 {
            assert!(
                (3..=5).contains(&flat[channel].abs_diff(ramp[channel])),
                "soft snow slope: {flat:?} {ramp:?}"
            );
        }
        let layer = mix::art_layer(source, 7, landscape::NATIVE_ICE, 1);
        let border = landscape::blend([layer; 3], [1.0, 0.0, 0.0], 1, 1 | 5 << 1);
        for channel in 0..4 {
            assert!(
                border[channel].abs_diff(ramp[channel]) <= 1,
                "pure/mixed snow seam: {border:?} {ramp:?}"
            );
        }
        assert_eq!(ramp[3], source[3]);
        assert_eq!(
            mix::art_layer(source, 8, landscape::NATIVE_ICE, 1),
            source.map(f32::from)
        );
    }
}
