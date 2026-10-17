//! One- and two-tile streams (Ford or river water) must draw as one continuous
//! band, and a Ford must not reuse the shoreline shallow sheet unchanged.
use super::threshold;
use crate::surface_mesh::{
    ProjectedSurfaceTriangle, appearance::water_material, apply_terrain_textures,
    projected_surface_triangles,
};
use crate::{SceneCamera, SceneTerrain, SceneTerrainAppearance, SceneTerrainSurface};
use std::collections::BTreeMap;
use wasm_bindgen_test::wasm_bindgen_test;

const DIRT: u8 = 2;
const SHALLOW: u8 = 10;
const FORD: u8 = 12;

fn tile(x: i32, y: i32, material: u8) -> SceneTerrain {
    let mut surface = SceneTerrainSurface::flat(0.0);
    surface.water = u8::from(material == SHALLOW) * 3;
    SceneTerrain {
        position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
        material,
        elevation_meters: 0.0,
        surface,
        appearance: Some(SceneTerrainAppearance {
            floor_strength: 0,
            canopy_strength: 0,
            palette: 5,
            exposure: 0,
            height_band: 0,
        }),
    }
}

fn mesh(water: &[[i32; 2]], material: u8) -> Vec<ProjectedSurfaceTriangle> {
    let terrain = (-8..8)
        .flat_map(|y| (-8..8).map(move |x| (x, y)))
        .map(|(x, y)| {
            tile(
                x,
                y,
                if water.contains(&[x, y]) {
                    material
                } else {
                    DIRT
                },
            )
        })
        .collect::<Vec<_>>();
    let camera = SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [1024.0, 768.0],
        focus_elevation_meters: 0.0,
    };
    projected_surface_triangles(&terrain, camera)
}

/// Water coverage the renderers draw at each displayed vertex of a tile.
fn coverage(faces: &[ProjectedSurfaceTriangle], at: [i32; 2]) -> BTreeMap<[i32; 2], f32> {
    let mut result = BTreeMap::new();
    for face in faces.iter().filter(|f| !f.skirt && f.tile == at) {
        let layers = face.texture_materials.expect("textured face");
        for (corner, point) in face.points.iter().enumerate() {
            // Bank corners may be bevelled by a quarter tile: key by nearest vertex.
            let key = [point.world[0], point.world[1]].map(|v| v.round() as i32);
            let value = if face.mix == [0; 6] {
                f32::from(u8::from(water_material(layers[0])))
            } else if water_material(layers[2]) {
                let share = f32::from(face.mix[corner]) / 255.0;
                threshold(share, share, 0.5)
            } else {
                0.0
            };
            result.insert(key, value);
        }
    }
    result
}

fn assert_continuous_band(chain: &[[i32; 2]], material: u8) {
    let faces = mesh(chain, material);
    let shared = |a: [i32; 2], b: [i32; 2]| {
        let (a, b) = (coverage(&faces, a), coverage(&faces, b));
        a.into_iter()
            .filter_map(|(key, value)| b.get(&key).map(|other| value.min(*other)))
            .collect::<Vec<_>>()
    };
    for &at in chain {
        let corners = coverage(&faces, at);
        assert_eq!(corners.len(), 4, "stream tile {at:?}");
        for (vertex, water) in corners {
            assert!(
                water >= 0.9,
                "material {material}: stream tile {at:?} is mostly land at vertex {vertex:?} ({water})"
            );
        }
    }
    for pair in chain.windows(2) {
        assert!(
            shared(pair[0], pair[1]).iter().any(|water| *water >= 0.9),
            "material {material}: stream breaks between {:?} and {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[wasm_bindgen_test]
fn straight_one_tile_stream_is_one_continuous_water_band() {
    let chain: Vec<[i32; 2]> = (-4..4).map(|y| [0, y]).collect();
    for material in [SHALLOW, FORD] {
        assert_continuous_band(&chain, material);
    }
}

#[wasm_bindgen_test]
fn diagonal_one_tile_stream_is_one_continuous_water_band() {
    let chain: Vec<[i32; 2]> = (-4..4).map(|i| [i, i]).collect();
    for material in [SHALLOW, FORD] {
        assert_continuous_band(&chain, material);
    }
}

#[wasm_bindgen_test]
fn staircase_stream_of_unequal_width_has_no_faded_necks() {
    // One-tile reaches joined by a two-tile pool, as a meandering stream samples.
    let mut chain: Vec<[i32; 2]> = (-4..-1).map(|y| [0, y]).collect();
    chain.extend([
        [0, -1],
        [1, -1],
        [0, 0],
        [1, 0],
        [1, 1],
        [1, 2],
        [2, 2],
        [2, 3],
    ]);
    for material in [SHALLOW, FORD] {
        assert_continuous_band(&chain, material);
    }
}

#[wasm_bindgen_test]
fn wide_water_keeps_its_half_covered_shore_vertices() {
    let lake: Vec<[i32; 2]> = (-3..3).flat_map(|y| (-3..3).map(move |x| [x, y])).collect();
    let faces = mesh(&lake, SHALLOW);
    let edge = coverage(&faces, [-3, 0]);
    assert!(edge[&[-3, 0]] > 0.4 && edge[&[-3, 0]] < 0.6, "{edge:?}");
    assert_eq!(coverage(&faces, [0, 0])[&[0, 0]], 1.0);
}

#[wasm_bindgen_test]
fn ford_interior_is_drawn_apart_from_the_shoreline_shallow_sheet() {
    let block: Vec<[i32; 2]> = (-3..3).flat_map(|y| (-3..3).map(move |x| [x, y])).collect();
    let art = crate::surface_mesh::water_fixture::art(true);
    let drawn = |material| {
        let mut faces = mesh(&block, material);
        apply_terrain_textures(&mut faces, &art);
        let face = faces
            .iter()
            .find(|f| !f.skirt && f.tile == [0, 0])
            .expect("interior face");
        (
            face.texture_uv,
            face.texture_blend,
            face.mix,
            face.floor_strengths,
            face.appearance,
        )
    };
    assert_ne!(
        drawn(FORD),
        drawn(SHALLOW),
        "a ford interior draws exactly like shallow shore water"
    );
}
