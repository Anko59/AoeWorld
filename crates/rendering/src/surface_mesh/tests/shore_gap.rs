//! Review F1/F2: the exact one-level bank gap and its source-halo blend.
use super::*;
use aoe_core::ScreenPoint;
use wasm_bindgen_test::wasm_bindgen_test;

pub(crate) const PROBE: [u32; 2] = [160, 32];
pub(crate) fn camera() -> SceneCamera {
    SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 2.0,
    }
}

pub(crate) fn scene(halo: bool) -> Vec<ProjectedSurfaceTriangle> {
    let tile = |x: i32, y: i32| {
        let wet = x >= 1;
        let height = if wet { 2 } else { 3 };
        SceneTerrain {
            position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
            material: if wet { 10 } else { 3 },
            elevation_meters: f64::from(height),
            appearance: halo.then_some(SceneTerrainAppearance {
                canopy_strength: 0,
                floor_strength: 0,
                palette: 5,
                exposure: 0,
                height_band: 0,
            }),
            surface: SceneTerrainSurface {
                corner_game_height_levels: [height; 4],
                kind: SceneTerrainSurface::PLATEAU,
                triangulation: 0,
                water: u8::from(wet),
            },
        }
    };
    let terrain = if halo {
        (-1..=1)
            .flat_map(|y| (-1..=2).map(move |x| tile(x, y)))
            .collect()
    } else {
        vec![tile(0, 0), tile(1, 0)]
    };
    let mut faces = projected_surface_triangles(&terrain, camera());
    apply_terrain_textures(&mut faces, &art());
    faces
}

pub(crate) fn assert_pixel(pixel: [u8; 4], halo: bool) {
    assert_eq!(pixel[3], 255, "bank probe must be opaque: {pixel:?}");
    if halo {
        assert!(
            pixel[0] > 35
                && pixel[0] < 225
                && pixel[1] > 95
                && pixel[1] < 195
                && pixel[2] > 145
                && pixel[2] < 195,
            "water blend must continue over the sand strip: {pixel:?}"
        );
    } else {
        for (actual, expected) in pixel.into_iter().zip([230_u8, 200, 140, 255]) {
            assert!(
                actual.abs_diff(expected) <= 2,
                "exact review probe must show neutral sand, not background: {pixel:?}"
            );
        }
    }
}

#[wasm_bindgen_test]
fn one_level_sand_water_bank_covers_the_review_probe_on_the_shared_mesh() {
    let probe = ScreenPoint { x: 160.0, y: 32.0 };
    let faces = scene(false);
    assert!(
        faces
            .iter()
            .any(|face| crate::surface_mesh::barycentric(face.points, probe).is_some()),
        "sand [3;4] and water [2;4] leave the review probe uncovered"
    );
    for face in faces.iter().filter(|face| face.skirt) {
        assert_eq!(face.material, 3);
        assert_eq!(face.tint, 0, "one-level sand banks have no dark wall shade");
    }
    let blended = scene(true);
    assert!(
        blended
            .iter()
            .filter(|face| face.skirt
                && face.tile == [0, 0]
                && face.points.iter().all(|point| point.world[0] == 1.0))
            .all(|face| face.texture_blend.is_some() && face.floor_strengths.is_some()),
        "complete shore support must blend over the closing strip"
    );
}
