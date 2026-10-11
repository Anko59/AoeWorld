//! Border probes shared by the three renderer pixel tests. Synthetic flat
//! frames only: any colour between two materials is the blend itself.
use super::{layer, noise, rank, weights};
#[path = "../tests/junction.rs"]
pub(crate) mod junction;
use crate::surface_mesh::{
    ProjectedSurfaceTriangle, apply_terrain_textures, landscape, projected_surface_triangles,
};
use crate::{
    AtlasAddress, GameArt, GameFrame, SceneCamera, SceneTerrain, SceneTerrainAppearance,
    SceneTerrainSurface,
};

/// Art groups: grass, dry grass, dirt, sand, (unused paving), water, forest floor.
const COLORS: [[u8; 4]; 7] = [
    [40, 170, 30, 255],
    [150, 150, 60, 255],
    [180, 130, 70, 255],
    [230, 200, 140, 255],
    [0, 0, 0, 0],
    [30, 90, 200, 255],
    [110, 65, 25, 255],
];
/// Columns of scene material, west to east: the last one continues eastwards.
/// Case 5 is a forest (material 6, full floor) against sand; case 6 has three
/// materials meeting within one tile column.
const STRIPS: [&[u8]; 7] = [
    &[0, 3],
    &[2, 4],
    &[7, 1],
    &[0, 8],
    &[9, 3],
    &[6, 3],
    &[2, 3, 0],
];
pub(crate) const CASES: usize = STRIPS.len();
pub(crate) const PROBES: [[u32; 2]; 10] = [
    [8, 64],
    [20, 64],
    [32, 64],
    [44, 64],
    [56, 64],
    [68, 64],
    [80, 64],
    [92, 64],
    [104, 64],
    [116, 64],
];

fn frame(group: usize) -> GameFrame {
    GameFrame {
        atlas: AtlasAddress {
            page: 0,
            uv: [2 + 8 * group as u32, 4, 5, 3].map(|value| value as f32 / 2048.0),
        },
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    }
}

pub(crate) fn art() -> GameArt {
    let mut terrain: [Vec<GameFrame>; 7] = Default::default();
    for group in [0, 1, 2, 3, 5, 6] {
        terrain[group] = vec![frame(group)];
    }
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: terrain[0].clone(),
        terrain,
        terrain_topology: [None; 7],
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
    }
}

pub(crate) fn atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for (group, color) in COLORS.iter().enumerate() {
        for y in 4..7 {
            for x in 2 + 8 * group..7 + 8 * group {
                atlas[(y * 2048 + x) * 4..][..4].copy_from_slice(color);
            }
        }
    }
    assert_eq!(crate::write_terrain_directory(&art(), &mut atlas), Ok(()));
    atlas
}

pub(crate) fn camera(case: usize) -> SceneCamera {
    SceneCamera {
        center: [if case == 6 { 0.6 } else { 0.1 }, -0.2],
        zoom: 0.5,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    }
}

/// The scene material of tile column `x`: strips start at column -1.
fn material(case: usize, x: i32) -> u8 {
    let strips = STRIPS[case];
    strips[(x + 1).clamp(0, strips.len() as i32 - 1) as usize]
}

pub(crate) fn terrain(case: usize) -> Vec<SceneTerrain> {
    (-8..8)
        .flat_map(|y| {
            (-8..8).map(move |x| {
                let material = material(case, x);
                SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material,
                    elevation_meters: 0.0,
                    surface: SceneTerrainSurface::flat(0.0),
                    appearance: Some(SceneTerrainAppearance {
                        canopy_strength: 0,
                        floor_strength: if material == 6 { 1000 } else { 0 },
                        palette: 5,
                        exposure: 0,
                        height_band: 0,
                    }),
                }
            })
        })
        .collect()
}

pub(crate) fn scene(case: usize) -> Vec<ProjectedSurfaceTriangle> {
    let mut faces = projected_surface_triangles(&terrain(case), camera(case));
    apply_terrain_textures(&mut faces, &art());
    faces
}

/// The flat colour a scene material shows on its own.
fn color(material: u8) -> [f32; 4] {
    let group = match material {
        4 | 7 | 9 => 2,
        8 => 5,
        6 => 0,
        material => usize::from(material),
    };
    layer(COLORS[group], material)
}

/// Independent of the mesh: columns depend on x only, so every share is the
/// linear interpolation of its two neighbouring vertex columns.
pub(crate) fn expected(case: usize, probe: usize) -> [u8; 4] {
    let camera = camera(case);
    let [x, y] = PROBES[probe];
    let world = aoe_core::Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: 0.0,
    }
    .screen_to_world(aoe_core::ScreenPoint {
        x: f64::from(x) + 0.5,
        y: f64::from(y) + 0.5,
    });
    let column = world[0].floor() as i32;
    let along = (world[0] - f64::from(column)) as f32;
    let quantized = |sum: u32| ((sum * 255 + 1) / 2) as u8;
    // Share of `wanted` at a vertex column: its two incident tile columns.
    let at = |vertex: i32, wanted: &dyn Fn(u8) -> bool| {
        f32::from(quantized(
            u32::from(wanted(material(case, vertex - 1)))
                + u32::from(wanted(material(case, vertex))),
        )) / 255.0
    };
    let share = |wanted: &dyn Fn(u8) -> bool| {
        at(column, wanted) + (at(column + 1, wanted) - at(column, wanted)) * along
    };
    let sheet = |material: u8| if material == 6 { 0 } else { material };
    let mut order = STRIPS[case].iter().map(|m| sheet(*m)).collect::<Vec<_>>();
    order.sort_by_key(|material| rank(*material));
    order.dedup();
    let tile = [column, world[1].floor() as i32];
    let noise = noise(tile, [along, (world[1] - world[1].floor()) as f32]);
    let over = order[order.len() - 1];
    let over_share = share(&|m| sheet(m) == over);
    // Three sheets take the middle layer; otherwise it is the forest-floor bed.
    let (middle, weights) = if order.len() == 3 {
        let middle_share = share(&|m| sheet(m) == order[1]);
        (
            color(order[1]),
            weights([over_share, middle_share, 0.0], true, noise),
        )
    } else {
        (
            layer(COLORS[6], 6),
            weights([over_share, 0.0, share(&|m| m == 6)], false, noise),
        )
    };
    landscape::blend(
        [color(order[0]), middle, color(over)],
        weights,
        0,
        1 | 5 << 1,
    )
}

pub(crate) fn assert_probe(case: usize, probe: usize, actual: [u8; 4], tolerance: u8) {
    let expected = expected(case, probe);
    for channel in 0..4 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= tolerance,
            "case {case} probe {:?}: {actual:?}, expected {expected:?}",
            PROBES[probe]
        );
    }
}

/// Along the probe row: both pure sheets appear, some pixels lie strictly
/// between them, and no neighbouring pixels jump as a hard tile edge would.
pub(crate) fn assert_soft(case: usize, pixel: &dyn Fn(u32, u32) -> [u8; 4]) {
    let row = (4..124).map(|x| pixel(x, 64)).collect::<Vec<_>>();
    let (first, last) = (row[0], row[row.len() - 1]);
    assert_ne!(first, last, "case {case}: two sheets");
    let span = (0..3)
        .map(|c| first[c].abs_diff(last[c]))
        .max()
        .unwrap_or(0);
    let between = row.iter().filter(|p| **p != first && **p != last).count();
    assert!(between >= 6, "case {case}: {between} blended pixels");
    for pair in row.windows(2) {
        let step = (0..3)
            .map(|c| pair[0][c].abs_diff(pair[1][c]))
            .max()
            .unwrap_or(0);
        assert!(
            u32::from(step) * 3 <= u32::from(span) * 2,
            "case {case}: hard step {pair:?}"
        );
    }
}
