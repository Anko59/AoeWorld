//! Ford coverage and substrate stay continuous across junctions and borders,
//! and the tuned 0.85 ford coverage and the thin deeper-water guard are pinned.
use super::narrow_stream_tests::tile;
use super::{support, threshold};
use crate::SceneCamera;
use crate::surface_mesh::{ProjectedSurfaceTriangle, projected_surface_triangles};
use wasm_bindgen_test::wasm_bindgen_test;

const FORD: u8 = 12;
/// Ford shore share in 255ths: shore coverage 0.85.
const FORD_SHARE: u8 = 150;

fn mesh(water: &[([i32; 2], u8)], land: impl Fn(i32, i32) -> u8) -> Vec<ProjectedSurfaceTriangle> {
    let terrain = (-8..8)
        .flat_map(|y| (-8..8).map(move |x| (x, y)))
        .map(|(x, y)| {
            let wet = water.iter().find(|(at, _)| *at == [x, y]);
            tile(x, y, wet.map_or_else(|| land(x, y), |(_, m)| *m))
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

#[wasm_bindgen_test]
fn ford_junction_with_mixed_land_has_no_opaque_seam_against_ordinary_ford_faces() {
    let ground = |x: i32, y: i32| [2, 0, 3, 7][((x * 7 + y * 13).rem_euclid(4)) as usize];
    let chain: Vec<([i32; 2], u8)> = (-4..4).map(|i| ([i, i], FORD)).collect();
    let faces = mesh(&chain, ground);
    let mut ford_faces = 0;
    for face in faces.iter().filter(|f| !f.skirt) {
        if let Some(packed) = support::packed(face) {
            assert!(
                !support::palette(packed).contains(&FORD),
                "junction face {:?} carries ford observations at full share",
                face.tile
            );
        }
        if face.texture_materials.is_some_and(|m| m[2] == FORD) && face.mix != [0; 6] {
            ford_faces += 1;
            for share in &face.mix[..3] {
                assert!(
                    *share <= FORD_SHARE,
                    "ford share {share} exceeds the ford cap"
                );
            }
        }
    }
    assert!(ford_faces > 0);
}

#[wasm_bindgen_test]
fn ford_interior_and_boundary_quads_share_one_substrate() {
    let block: Vec<([i32; 2], u8)> = (-3..3)
        .flat_map(|y| (-3..3).map(move |x| ([x, y], FORD)))
        .collect();
    let faces = mesh(&block, |_, _| 2);
    let layers = |at: [i32; 2]| {
        faces
            .iter()
            .find(|f| !f.skirt && f.tile == at && f.mix != [0; 6])
            .and_then(|f| f.texture_materials)
            .expect("mixed ford face")
    };
    let (interior, boundary) = (layers([0, 0]), layers([-3, 0]));
    assert_eq!(interior[2], FORD);
    assert_eq!(
        interior[0], boundary[0],
        "sand-backed interior meets a dirt-backed boundary"
    );
}

#[wasm_bindgen_test]
fn ford_interior_coverage_is_pinned_at_085() {
    let block: Vec<([i32; 2], u8)> = (-3..3)
        .flat_map(|y| (-3..3).map(move |x| ([x, y], FORD)))
        .collect();
    let faces = mesh(&block, |_, _| 2);
    let face = faces
        .iter()
        .find(|f| !f.skirt && f.tile == [0, 0])
        .expect("interior face");
    assert_eq!(face.mix[..3], [FORD_SHARE; 3]);
    let share = f32::from(FORD_SHARE) / 255.0;
    let coverage = threshold(share, share, 0.5);
    assert!((coverage - 0.85).abs() < 0.02, "ford coverage {coverage}");
}

#[wasm_bindgen_test]
fn thin_deeper_water_inside_a_ford_starts_from_the_shallow_sheet() {
    let mut stream: Vec<([i32; 2], u8)> = (-4..4).map(|y| ([0, y], FORD)).collect();
    stream[4].1 = 5;
    let faces = mesh(&stream, |_, _| 2);
    let deeper = faces
        .iter()
        .filter(|f| !f.skirt && f.tile == [0, 0])
        .collect::<Vec<_>>();
    assert_eq!(deeper.len(), 2);
    for face in deeper {
        let layers = face.texture_materials.expect("textured face");
        assert!(
            layers.iter().all(|m| matches!(m, 10 | 12)),
            "medium-blue triangle in a ford: {layers:?}"
        );
    }
}
