//! World-anchored tiling probes shared by the three renderer pixel tests.
//! Synthetic frames only: every texel names its frame and its own position.
use crate::surface_mesh::{
    ProjectedSurfaceTriangle, apply_terrain_textures, projected_surface_triangles,
};
use crate::{
    AtlasAddress, GameArt, GameFrame, SceneCamera, SceneTerrain, SceneTerrainAppearance,
    SceneTerrainSurface, TerrainTopology,
};

const SIZE: [u32; 2] = [5, 3];
const SHEET: [u16; 2] = [4, 3];
const ACCENTS: usize = 5;
pub(crate) const CASES: usize = 5;
// Chosen offline: every probe is clear of tile and texel edges at both zooms,
// in distinct far-zoom tiles at every position inside a coarse texture patch.
pub(crate) const PROBES: [[u32; 2]; 10] = [
    [26, 58],
    [28, 72],
    [29, 48],
    [42, 46],
    [46, 74],
    [48, 57],
    [56, 82],
    [57, 58],
    [78, 50],
    [81, 40],
];

/// Expected RGBA and whether blue (the in-frame texel position) is exact.
pub(crate) type Expected = Option<([u8; 4], bool)>;

fn origin(material: u8, index: usize) -> (u32, [u32; 2]) {
    if material == 0 {
        (0, [2 + 10 * index as u32, 4])
    } else {
        (1, [6 + 10 * index as u32, 12])
    }
}

fn texel(material: u8, index: usize, at: [u32; 2]) -> [u8; 4] {
    [
        30 + 18 * index as u8,
        if material == 0 { 40 } else { 200 },
        10 + 40 * at[0] as u8 + 15 * at[1] as u8,
        255,
    ]
}

fn frames(material: u8, count: usize) -> Vec<GameFrame> {
    (0..count)
        .map(|index| {
            let (page, [x, y]) = origin(material, index);
            GameFrame {
                atlas: AtlasAddress {
                    page,
                    uv: [x, y, SIZE[0], SIZE[1]].map(|value| value as f32 / 2048.0),
                },
                size: [96.0, 48.0],
                anchor: [48.0, 24.0],
            }
        })
        .collect()
}

/// Grass is a declared 4×3 periodic sheet on page 0; dirt is five
/// coordinate-stable accents on page 1. Every other group is absent.
pub(crate) fn art() -> GameArt {
    let grass = frames(0, usize::from(SHEET[0] * SHEET[1]));
    let mut terrain: [Vec<GameFrame>; 11] = Default::default();
    terrain[0] = grass.clone();
    terrain[2] = frames(2, ACCENTS);
    let mut terrain_topology = [None; 11];
    terrain_topology[0] = Some(TerrainTopology::PeriodicXMajorReversedY {
        columns: SHEET[0],
        rows: SHEET[1],
    });
    terrain_topology[2] = Some(TerrainTopology::CoordinateStableAccents);
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass,
        terrain,
        terrain_topology,
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
    }
}

/// Resolves a world tile through the written atlas directory, as shaders do.
pub(crate) fn frame(atlas: &[u8], material: u8, tile: [i32; 2]) -> Option<AtlasAddress> {
    super::sheet(atlas, material).map(|sheet| sheet.frame(atlas, tile))
}

pub(crate) fn atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for (material, count) in [(0, usize::from(SHEET[0] * SHEET[1])), (2, ACCENTS)] {
        for index in 0..count {
            let (page, [x, y]) = origin(material, index);
            for ty in 0..SIZE[1] {
                for tx in 0..SIZE[0] {
                    let start = page as usize * crate::GAME_ATLAS_PAGE_BYTES
                        + ((y + ty) as usize * 2048 + (x + tx) as usize) * 4;
                    atlas[start..start + 4].copy_from_slice(&texel(material, index, [tx, ty]));
                }
            }
        }
    }
    assert_eq!(crate::write_terrain_directory(&art(), &mut atlas), Ok(()));
    atlas
}

pub(crate) fn camera(case: usize) -> SceneCamera {
    SceneCamera {
        center: [0.37, -0.21],
        zoom: if case == 0 { 0.5 } else { 1.0 / 40.0 },
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    }
}

/// The frame a world tile owns at every zoom and mesh LOD.
fn frame_index(material: u8, tile: [i32; 2]) -> usize {
    if material == 0 {
        let column = tile[0].rem_euclid(i32::from(SHEET[0])) as usize;
        let rows = i32::from(SHEET[1]);
        let row = (rows - tile[1].rem_euclid(rows)).rem_euclid(rows) as usize;
        column * usize::from(SHEET[1]) + row
    } else {
        (tile[0] * 7 + tile[1] * 13).unsigned_abs() as usize % ACCENTS
    }
}

/// Independent reference: a world point selects its own tile's frame and the
/// native diamond texel there. Points near a tile or texel edge are ambiguous.
fn reference(material: u8, world: [f64; 2]) -> Option<[u8; 4]> {
    let tile = world.map(f64::floor);
    let local = [world[0] - tile[0], world[1] - tile[1]];
    let diamond = [
        0.5 + 0.5 * (local[0] - local[1]),
        0.5 * (local[0] + local[1]),
    ];
    let mut at = [0; 2];
    for axis in 0..2 {
        let position = 0.5 + diamond[axis] * f64::from(SIZE[axis] - 1);
        if !(0.06..0.94).contains(&local[axis]) || !(0.1..0.9).contains(&position.fract()) {
            return None;
        }
        at[axis] = position as u32;
    }
    let index = frame_index(material, [tile[0] as i32, tile[1] as i32]);
    Some(texel(material, index, at))
}

/// 0: grass, near zoom, one face per tile. 1: grass, far zoom, coarse patches.
/// 2: dirt, far zoom, few tiles so the mesh stays fine. 3: dirt, far zoom,
/// coarse. 4: grass with a full forest floor, far zoom, coarse, filtered bed.
pub(crate) fn scene(case: usize) -> Vec<ProjectedSurfaceTriangle> {
    let material = if matches!(case, 2 | 3) { 2 } else { 0 };
    // Few tiles keep one face per tile; the wide map needs 2×2-tile texture patches.
    let reach = if matches!(case, 0 | 2) { 24 } else { 64 };
    let appearance = (case == 4).then_some(SceneTerrainAppearance {
        canopy_strength: 0,
        floor_strength: 1000,
        palette: 5,
        exposure: 0,
        height_band: 0,
    });
    let terrain = (-reach..reach)
        .flat_map(|y| {
            (-reach..reach).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance,
            })
        })
        .collect::<Vec<_>>();
    let camera = camera(case);
    let mut faces = projected_surface_triangles(&terrain, camera);
    let coarse = faces.iter().any(|face| {
        let xs = face.points.map(|point| point.world[0]);
        xs.iter().fold(f64::MIN, |a, b| a.max(*b)) - xs.iter().fold(f64::MAX, |a, b| a.min(*b))
            > 1.0
    });
    assert_eq!(coarse, matches!(case, 1 | 3 | 4), "mesh LOD premise");
    apply_terrain_textures(&mut faces, &art());
    assert!(
        expected(case).iter().all(Option::is_some),
        "ambiguous probe in case {case}"
    );
    faces
}

pub(crate) fn assert_probe(case: usize, probe: usize, actual: [u8; 4], tolerance: u8) {
    let Some((expected, blue)) = expected(case)[probe] else {
        return;
    };
    for channel in 0..4 {
        if channel == 2 && !blue {
            continue;
        }
        assert!(
            actual[channel].abs_diff(expected[channel]) <= tolerance,
            "case {case} probe {:?}: {actual:?}, expected {expected:?}",
            PROBES[probe]
        );
    }
}

/// A full forest floor shows only the bed (dirt without forest art). Its
/// minified taps stay inside the tile's own frame, so frame identity is exact.
pub(crate) fn expected(case: usize) -> [Expected; PROBES.len()] {
    let camera = camera(case);
    let projection = aoe_core::Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: 0.0,
    };
    PROBES.map(|[x, y]| {
        let world = projection.screen_to_world(aoe_core::ScreenPoint {
            x: f64::from(x) + 0.5,
            y: f64::from(y) + 0.5,
        });
        let material = if matches!(case, 2 | 3 | 4) { 2 } else { 0 };
        reference(material, world).map(|texel| (texel, case != 4))
    })
}
