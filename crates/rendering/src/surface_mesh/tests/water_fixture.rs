//! Water depth fixture rendered by Canvas, WebGL2 and WebGPU. Each class owns
//! one synthetic sheet colour (never private art); a face between two classes
//! must show their barycentric blend and no procedural flat tint.
use crate::surface_mesh::{
    ProjectedSurfaceTriangle, SurfacePoint, apply_terrain_textures, terrain_texture_frame,
};
use crate::{AtlasAddress, GameArt, GameFrame};

/// Scene materials of shallow water, medium water, deep water and fords.
pub(crate) const SHALLOW: u8 = 10;
pub(crate) const MEDIUM: u8 = 5;
pub(crate) const DEEP: u8 = 11;
pub(crate) const FORD: u8 = 12;
pub(crate) const CLASSES: [(u8, [u8; 4]); 4] = [
    (SHALLOW, [60, 190, 200, 255]),
    (MEDIUM, [20, 90, 180, 255]),
    (DEEP, [5, 40, 120, 255]),
    (FORD, [60, 190, 200, 255]),
];
const LAND: [u8; 4] = [200, 40, 160, 255];
pub(crate) const PROBES: [[u32; 2]; 3] = [[32, 48], [80, 48], [48, 48]];

/// Colour block `index`: 4×4 texels below the atlas rows reserved for the
/// terrain frame directory, addressed by its inner texel.
fn frame(index: usize) -> GameFrame {
    GameFrame {
        atlas: AtlasAddress {
            page: 0,
            uv: [
                (index as f32 * 8.0 + 1.0) / 2048.0,
                9.0 / 2048.0,
                1.0 / 2048.0,
                1.0 / 2048.0,
            ],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    }
}

pub(crate) fn atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    let colors = CLASSES
        .iter()
        .enumerate()
        .map(|(i, class)| if i == 3 { [70, 150, 90, 255] } else { class.1 })
        .chain([LAND]);
    for (index, color) in colors.enumerate() {
        for y in 0..4 {
            for x in 0..4 {
                let start = ((y + 8) * 2048 + index * 8 + x) * 4;
                atlas[start..start + 4].copy_from_slice(&color);
            }
        }
    }
    atlas
}

/// Shallow water and fords use the required water sheet. Optional medium,
/// deep and unused marsh sheets keep distinct colours to detect a wrong slot.
/// `depth` false models a pack without those optional sheets.
pub(crate) fn art(depth: bool) -> GameArt {
    GameArt {
        actors: Vec::new(),
        buildings: Default::default(),
        village: Default::default(),
        town_center: Default::default(),
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame(4)],
        terrain: std::array::from_fn(|slot| vec![frame(if slot == 5 { 0 } else { 4 })]),
        terrain_topology: [None; 12],
        water_depth: std::array::from_fn(|slot| {
            if depth {
                (vec![frame(slot + 1)], None)
            } else {
                (Vec::new(), None)
            }
        }),
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
    }
}

fn color(material: u8) -> [u8; 4] {
    CLASSES
        .iter()
        .find(|class| class.0 == material)
        .expect("water class")
        .1
}

/// A 96-pixel quad whose left vertices are `left` water and right vertices
/// `right` water, textured through the shared assignment.
pub(crate) fn faces(left: u8, right: u8) -> [ProjectedSurfaceTriangle; 2] {
    let art = art(true);
    let points =
        [[16.0, 16.0], [112.0, 16.0], [112.0, 112.0], [16.0, 112.0]].map(|p| SurfacePoint {
            world: [0.0; 3],
            screen: aoe_core::ScreenPoint { x: p[0], y: p[1] },
        });
    let mut faces = [[0, 1, 2], [0, 2, 3]].map(|indices| ProjectedSurfaceTriangle {
        points: indices.map(|i| points[i]),
        color: [0.0; 3],
        tile: [0; 2],
        skirt: false,
        material: left,
        appearance: 0,
        floor_strengths: None,
        mix: [0; 6],
        texture_mode: 0,
        tint: 0,
        texture_uv: None,
        texture_blend: None,
        texture_tile: [0; 2],
        texture_materials: Some(indices.map(|i| if i == 1 || i == 2 { right } else { left })),
        pickable: true,
        order: 0,
    });
    apply_terrain_textures(&mut faces, &art);
    for face in &faces {
        let own = terrain_texture_frame(&art, left, [0; 2]).map(|frame| frame.atlas);
        assert_eq!(face.tint, 0, "water is never procedurally tinted");
        assert_eq!(face.texture_uv, own);
        assert_eq!(face.texture_blend.is_some(), left != right);
    }
    faces
}

pub(crate) fn expected(left: u8, right: u8, x: u32) -> [u8; 4] {
    let share = (x as f32 + 0.5 - 16.0) / 96.0;
    let (left, right) = (color(left), color(right));
    std::array::from_fn(|c| {
        ((1.0 - share) * f32::from(left[c]) + share * f32::from(right[c])).round() as u8
    })
}

/// Every ordered pair the backends render: uniform classes and their borders.
pub(crate) fn pairs() -> [(u8, u8); 8] {
    [
        (SHALLOW, SHALLOW),
        (MEDIUM, MEDIUM),
        (DEEP, DEEP),
        (FORD, FORD),
        (SHALLOW, MEDIUM),
        (MEDIUM, DEEP),
        (FORD, MEDIUM),
        (DEEP, SHALLOW),
    ]
}
