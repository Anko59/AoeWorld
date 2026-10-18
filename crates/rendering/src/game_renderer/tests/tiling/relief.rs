//! Slope shading shared by the three renderer pixel tests: ground facing the
//! light is brighter, ground facing away darker, flat ground and the zoom
//! levels agree. Every texel is one constant colour, so any change is shading.
use crate::surface_mesh::directory::fixture;
use crate::surface_mesh::{ProjectedSurfaceTriangle, apply_terrain_textures};
use crate::{GAME_ATLAS_PAGE_BYTES, SceneCamera, SceneTerrain, SceneTerrainSurface};

pub(crate) const TEXEL: [u8; 4] = [90, 150, 60, 255];
/// Five probes well inside the 128 px target, clear of its edges.
pub(crate) const PROBES: [[u32; 2]; 5] = [[64, 64], [48, 56], [80, 56], [52, 76], [76, 76]];

pub(crate) fn camera(far: bool) -> SceneCamera {
    fixture::camera(usize::from(far))
}

/// The directory fixture's atlas with every grass texel replaced by `TEXEL`.
pub(crate) fn atlas() -> Vec<u8> {
    let art = fixture::art();
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for frame in &art.terrain[0] {
        let [x, y, width, height] = frame
            .atlas
            .uv
            .map(|value| (value * 2048.0).round() as usize);
        for row in y..y + height {
            for column in x..x + width {
                let start =
                    frame.atlas.page as usize * GAME_ATLAS_PAGE_BYTES + (row * 2048 + column) * 4;
                atlas[start..start + 4].copy_from_slice(&TEXEL);
            }
        }
    }
    assert_eq!(crate::write_terrain_directory(&art, &mut atlas), Ok(()));
    atlas
}

/// Grass whose height rises by `rise` levels per tile toward the east (+1
/// faces the north-west light, -1 faces away, 0 is flat). Every tile is one
/// planar one-level ramp, so the near mesh keeps every source tile and the
/// coarse far mesh averages to the same plane.
pub(crate) fn faces(far: bool, rise: i16) -> Vec<ProjectedSurfaceTriangle> {
    faces_with(far, |x, _y| rise * x as i16)
}

/// Grass over any corner-height field (levels at integer tile corners).
pub(crate) fn faces_with(
    far: bool,
    level: impl Fn(i32, i32) -> i16,
) -> Vec<ProjectedSurfaceTriangle> {
    faces_in(far, false, level)
}

/// `wide` selects a large mesh viewport and reach: gentle, low terrain only.
pub(crate) fn faces_in(
    far: bool,
    wide: bool,
    level: impl Fn(i32, i32) -> i16,
) -> Vec<ProjectedSurfaceTriangle> {
    let reach = [[24, 48], [64, 64]][usize::from(far)][usize::from(wide)];
    let mut faces = mesh(far, wide, &terrain(reach, level));
    apply_terrain_textures(&mut faces, &fixture::art());
    faces
}

/// The same ground without textures: flat grass-coloured fills.
pub(crate) fn untextured_faces(
    far: bool,
    wide: bool,
    level: impl Fn(i32, i32) -> i16,
) -> Vec<ProjectedSurfaceTriangle> {
    mesh(
        far,
        wide,
        &terrain(
            [[24, 48], [64, 64]][usize::from(far)][usize::from(wide)],
            level,
        ),
    )
}

/// The projected mesh, cropped to the 128 px target.
fn mesh(far: bool, wide: bool, terrain: &[SceneTerrain]) -> Vec<ProjectedSurfaceTriangle> {
    let camera = camera(far);
    // The far mesh is selected on a wide viewport, as the tiling fixture does.
    let mut mesh_camera = camera;
    if far {
        mesh_camera.viewport = [512.0; 2];
    } else if wide {
        mesh_camera.viewport = [1024.0; 2];
    }
    let mut faces = crate::surface_mesh::projected_surface_triangles(terrain, mesh_camera);
    for face in &mut faces {
        for point in &mut face.points {
            point.screen.x -= (mesh_camera.viewport[0] - camera.viewport[0]) / 2.0;
            point.screen.y -= (mesh_camera.viewport[1] - camera.viewport[1]) / 2.0;
        }
    }
    faces
}

/// A slope facing the light in both axes (`2x + y` levels) over terrain missing
/// its north-east quadrant, so the fit windows next to it are clipped on one
/// side. Returns the faces and five probes inside tiles beside the gap.
pub(crate) fn clipped_slope() -> (Vec<ProjectedSurfaceTriangle>, [[u32; 2]; 5]) {
    let terrain = terrain(24, |x, y| (2 * x + y) as i16)
        .into_iter()
        .filter(|tile| !(tile.position[0] > 0.0 && tile.position[1] < 0.0))
        .collect::<Vec<_>>();
    let mut faces = mesh(false, false, &terrain);
    apply_terrain_textures(&mut faces, &fixture::art());
    let mut probes = faces
        .iter()
        .filter(|face| {
            !face.skirt && (0..=2).contains(&face.tile[0]) && (0..=2).contains(&face.tile[1])
        })
        .map(|face| {
            let screen = face.points.map(|point| point.screen);
            [
                (screen.iter().map(|p| p.x).sum::<f64>() / 3.0).round(),
                (screen.iter().map(|p| p.y).sum::<f64>() / 3.0).round(),
            ]
        })
        .filter(|[x, y]| (8.0..120.0).contains(x) && (8.0..120.0).contains(y))
        .map(|[x, y]| [x as u32, y as u32])
        .collect::<Vec<_>>();
    assert!(
        probes.len() >= 3,
        "clipped-window premise: {} probes",
        probes.len()
    );
    probes.truncate(5);
    let first = probes[0];
    probes.resize(5, first);
    (faces, probes.try_into().unwrap())
}

/// Ground beside a gap in the terrain, on a slope facing the light, must be
/// brighter than flat ground: missing cells may not turn the slope dark.
pub(crate) fn assert_clipped_lit(probes: &[[u8; 4]; 5]) {
    let lit = ratio(probes);
    assert!(
        lit >= 1.08,
        "clipped window: slope facing the light {lit:.3} of flat: {probes:?}"
    );
}

/// Grass tiles over `-reach..reach` whose corners follow `level`.
fn terrain(reach: i32, level: impl Fn(i32, i32) -> i16) -> Vec<SceneTerrain> {
    let level = &level;
    (-reach..reach)
        .flat_map(|y| {
            (-reach..reach).map(move |x| {
                let corners = [
                    level(x, y),
                    level(x + 1, y),
                    level(x + 1, y + 1),
                    level(x, y + 1),
                ];
                let low = *corners.iter().min().unwrap();
                let high = *corners.iter().max().unwrap();
                let kind = if low == high {
                    SceneTerrainSurface::PLATEAU
                } else {
                    SceneTerrainSurface::RAMP
                };
                SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 0,
                    elevation_meters: f64::from(low),
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: corners,
                        kind,
                        triangulation: u8::from(corners[0] + corners[2] > corners[1] + corners[3]),
                        water: 0,
                    },
                    appearance: None,
                }
            })
        })
        .collect::<Vec<_>>()
}

fn mean(pixels: &[[u8; 4]]) -> f64 {
    let sum: u32 = pixels
        .iter()
        .flat_map(|p| p[..3].iter().map(|v| u32::from(*v)))
        .sum();
    f64::from(sum) / (3.0 * pixels.len() as f64)
}

/// Ramp tiles are drawn 8 % darker whatever they face (the existing ramp tint);
/// slope shading is measured against that baseline.
const RAMP_TINT: f64 = 0.92;

/// Mean brightness of the probes relative to the constant texel's, with the
/// ramp tint divided out.
pub(crate) fn ratio(pixels: &[[u8; 4]]) -> f64 {
    mean(pixels) / mean(&[TEXEL]) / RAMP_TINT
}

/// Flat ground stays the texel; a slope facing the light is at least 8 %
/// brighter and one facing away at least 8 % darker, at near and far zoom
/// alike, and the two zooms agree within 12 points.
pub(crate) fn assert_directional(
    far: bool,
    flat: &[[u8; 4]],
    toward: &[[u8; 4]],
    away: &[[u8; 4]],
) -> [f64; 2] {
    for (index, pixel) in flat.iter().enumerate() {
        assert!(
            pixel.iter().zip(TEXEL).all(|(a, b)| a.abs_diff(b) <= 1),
            "flat probe {index} (far {far}) must stay {TEXEL:?}: {pixel:?}"
        );
    }
    let (lit, dark) = (ratio(toward), ratio(away));
    assert!(
        lit >= 1.08,
        "far {far}: slope facing the light {lit:.3} of flat: {toward:?} away {away:?}"
    );
    assert!(
        dark <= 0.92,
        "far {far}: slope facing away {dark:.3} of flat"
    );
    [lit, dark]
}

pub(crate) fn assert_zoom_agreement(near: [f64; 2], far: [f64; 2]) {
    assert!(
        (near[0] - far[0]).abs() <= 0.12 && (near[1] - far[1]).abs() <= 0.12,
        "slope shading must agree across zoom: near {near:?}, far {far:?}"
    );
}

/// Rolling hills as gentle as the real Large map: three levels over an 88
/// tile period, slopes up to 0.3 levels per tile.
pub(crate) fn hills(x: i32, y: i32) -> i16 {
    (3.0 * ((f64::from(x) + f64::from(y)) / 14.0).sin()).round() as i16
}

/// Steeper, shorter hills for the near view, which sees only two tiles.
fn hills_near(x: i32, y: i32) -> i16 {
    (2.0 * ((f64::from(x) + f64::from(y)) / 3.0).sin()).round() as i16
}

fn vertex_key(point: &crate::surface_mesh::SurfacePoint) -> [i64; 2] {
    [point.world[0], point.world[1]].map(|v| (v * 1024.0).round() as i64)
}

fn steps(face: &ProjectedSurfaceTriangle) -> f64 {
    face.relief().iter().map(|c| f64::from(*c)).sum::<f64>() / 3.0
}

/// Two ground faces sharing an edge across which the mean shade differs most,
/// with the five pixels (two each side) straddling the edge midpoint. Flat
/// per-face shading would step there; per-vertex shading must not.
pub(crate) fn seam(
    far: bool,
    textured: bool,
) -> (Vec<ProjectedSurfaceTriangle>, [[u32; 2]; 5], f64) {
    let level = if far { hills } else { hills_near };
    let faces = if textured {
        faces_in(far, true, level)
    } else {
        untextured_faces(far, true, level)
    };
    let mut edges = std::collections::BTreeMap::<([i64; 2], [i64; 2]), Vec<usize>>::new();
    for (index, face) in faces.iter().enumerate().filter(|(_, face)| !face.skirt) {
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let (a, b) = (vertex_key(&face.points[a]), vertex_key(&face.points[b]));
            edges.entry((a.min(b), a.max(b))).or_default().push(index);
        }
    }
    let mut best = None;
    for pair in edges.values().filter(|faces| faces.len() == 2) {
        let (a, b) = (&faces[pair[0]], &faces[pair[1]]);
        let shared = a
            .points
            .iter()
            .filter(|p| b.points.iter().any(|q| vertex_key(p) == vertex_key(q)))
            .collect::<Vec<_>>();
        let (x, y) = (
            (shared[0].screen.x + shared[1].screen.x) / 2.0,
            (shared[0].screen.y + shared[1].screen.y) / 2.0,
        );
        let gap = (steps(a) - steps(b)).abs();
        if (12.0..116.0).contains(&x)
            && (12.0..116.0).contains(&y)
            && best.is_none_or(|(g, _, _)| gap > g)
        {
            best = Some((gap, x, y));
        }
    }
    let (gap, x, y) = best.expect("a shared edge inside the target");
    let (x, y) = (x.round() as u32, y.round() as u32);
    (
        faces,
        [-2, -1, 0, 1, 2].map(|d| [(x as i32 + d) as u32, y]),
        gap,
    )
}

/// Neighbouring pixels across the seam differ by at most two levels, and the
/// premise holds: the faces' mean shades differ by at least 0.3 steps, so the seam sits on a real gradient.
pub(crate) fn assert_continuous(far: bool, gap: f64, probes: &[[u8; 4]; 5]) {
    assert!(
        gap >= 1.0,
        "far {far}: seam premise, shade gap {gap:.2} steps"
    );
    for pair in probes.windows(2) {
        for channel in 0..3 {
            assert!(
                pair[0][channel].abs_diff(pair[1][channel]) <= 2,
                "far {far}: shade steps across a shared edge (gap {gap:.2}): {probes:?}"
            );
        }
    }
}

/// Every lattice corner carries one code whichever face reads it.
#[wasm_bindgen_test::wasm_bindgen_test]
fn shared_vertices_carry_identical_shade_at_near_and_far_zoom() {
    for far in [false, true] {
        let mut seen = std::collections::BTreeMap::new();
        let mut varied = std::collections::BTreeSet::new();
        for face in faces_in(far, true, hills).iter().filter(|face| !face.skirt) {
            for (point, code) in face.points.iter().zip(face.relief()) {
                varied.insert(code);
                let first = *seen.entry(vertex_key(point)).or_insert(code);
                assert_eq!(
                    first,
                    code,
                    "far {far}: vertex {:?} differs",
                    vertex_key(point)
                );
            }
        }
        assert!(varied.len() >= 5, "far {far}: shade premise {varied:?}");
    }
}

/// One-level terraces four tiles wide climbing toward the east shade as a
/// hillside: a face's three corners differ by at most three steps (12 %).
#[wasm_bindgen_test::wasm_bindgen_test]
fn terrace_steps_shade_as_a_smooth_hillside_without_tents() {
    let faces = faces_with(false, |x, _y| x.div_euclid(4) as i16);
    let ground = faces.iter().filter(|face| !face.skirt).collect::<Vec<_>>();
    assert!(
        ground.len() >= 16,
        "terrace premise: {} faces",
        ground.len()
    );
    for face in ground {
        let codes = face.relief();
        let spread = codes.iter().max().unwrap() - codes.iter().min().unwrap();
        assert!(
            spread <= 3,
            "tile {:?}: corners {codes:?} make a tent",
            face.tile
        );
    }
}

/// Rolling hills as gentle as the real map still shade visibly (a 6-step,
/// 24 % spread of vertex codes) and average near neutral.
#[wasm_bindgen_test::wasm_bindgen_test]
fn gentle_rolling_hills_still_shade_visibly_and_stay_neutral_on_average() {
    let camera = SceneCamera {
        center: [0.37, -0.21],
        zoom: 0.5,
        viewport: [2048.0, 1024.0],
        focus_elevation_meters: 0.0,
    };
    let faces = crate::surface_mesh::projected_surface_triangles(&terrain(90, hills), camera);
    let codes = faces
        .iter()
        .filter(|face| !face.skirt)
        .flat_map(|face| face.relief())
        .map(i32::from)
        .collect::<Vec<_>>();
    assert!(codes.len() > 3000, "hill premise: {} corners", codes.len());
    let (low, high) = (codes.iter().min().unwrap(), codes.iter().max().unwrap());
    assert!(high - low >= 6, "gentle hills barely shade: {low}..{high}");
    let mean = codes.iter().sum::<i32>() as f64 / codes.len() as f64;
    assert!(
        mean.abs() <= 1.0,
        "average shade {mean:.2} steps must stay near neutral"
    );
}
