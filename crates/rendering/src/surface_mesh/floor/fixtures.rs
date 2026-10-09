//! Pixel fixtures shared by three actual renderer tests; not rendering evidence itself.
use super::*;

pub(crate) fn faces(interpolated: bool) -> [ProjectedSurfaceTriangle; 2] {
    let address = |page| crate::AtlasAddress {
        page,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    let points =
        [[16.0, 16.0], [112.0, 16.0], [112.0, 112.0], [16.0, 112.0]].map(|p| SurfacePoint {
            world: [0.0; 3],
            screen: aoe_core::ScreenPoint { x: p[0], y: p[1] },
        });
    [[0, 1, 2], [0, 2, 3]].map(|indices| ProjectedSurfaceTriangle {
        points: indices.map(|i| points[i]),
        color: [0.0; 3],
        tile: [0; 2],
        skirt: false,
        material: 2,
        appearance: landscape::pack(Some(crate::SceneTerrainAppearance {
            floor_strength: 650,
            canopy_strength: 0,
            palette: 5,
            exposure: 0,
            height_band: 0,
        })),
        floor_strengths: interpolated
            .then(|| indices.map(|i| if i == 1 || i == 2 { 255 } else { 0 })),
        texture_mode: 0,
        tint: 0,
        texture_uv: Some(address(0)),
        texture_blend: Some([address(1), address(2)]),
        texture_tile: [0; 2],
        texture_materials: None,
        pickable: true,
        order: 0,
    })
}

pub(crate) const PROBES: [[u32; 2]; 3] = [[32, 48], [80, 48], [48, 48]];
pub(crate) fn expected(x: u32, interpolated: bool) -> [u8; 4] {
    let floor = if interpolated {
        (x as f32 + 0.5 - 16.0) / 96.0
    } else {
        0.65
    };
    [
        ((1.0 - floor) * 255.0).round() as u8,
        (floor * 255.0).round() as u8,
        0,
        255,
    ]
}
// Deliberately distinct synthetic grass/dirt/leaf-litter colors, not private art.
pub(crate) const FOREST_COLORS: [[u8; 4]; 3] =
    [[40, 170, 30, 255], [180, 130, 70, 255], [110, 65, 25, 255]];
pub(crate) const FOREST_PROBES: [[u32; 2]; 5] = [[16, 48], [32, 48], [80, 48], [48, 48], [111, 48]];

pub(crate) fn forest_atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for (page, color) in FOREST_COLORS.iter().enumerate() {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        atlas[start..start + 4].copy_from_slice(color);
    }
    atlas
}

pub(crate) fn forest_faces(
    material: u8,
    floor: u16,
    gradient: bool,
) -> [ProjectedSurfaceTriangle; 2] {
    let frame = |page| crate::GameFrame {
        atlas: crate::AtlasAddress {
            page,
            uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let art = crate::GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame(0)],
        terrain: std::array::from_fn(|m| {
            vec![frame(match m {
                2 => 1,
                6 => 2,
                _ => 0,
            })]
        }),
        terrain_topology: [None; 7],
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
    };
    let mut faces = faces(gradient);
    for face in &mut faces {
        face.material = material;
        face.appearance = landscape::pack(Some(crate::SceneTerrainAppearance {
            floor_strength: floor,
            canopy_strength: 0,
            palette: 5,
            exposure: 0,
            height_band: 0,
        }));
    }
    apply_terrain_textures(&mut faces, &art);
    for face in &faces {
        let primary = if material == 2 { 1 } else { 0 };
        assert_eq!(face.texture_uv, Some(frame(primary).atlas));
        assert_eq!(
            face.texture_blend,
            Some([frame(2).atlas, frame(primary).atlas])
        );
        assert_eq!(
            face.texture_materials,
            Some([
                if material == 2 { 2 } else { 0 },
                6,
                if material == 2 { 2 } else { 0 }
            ])
        );
        if gradient {
            assert!(face.floor_strengths.is_some());
        }
    }
    faces
}

pub(crate) fn forest_expected(material: u8, floor: u16, gradient: bool, x: u32) -> [u8; 4] {
    let strength = if gradient {
        (x as f32 + 0.5 - 16.0) / 96.0
    } else {
        f32::from(floor) / 1000.0
    };
    let primary = FOREST_COLORS[usize::from(material == 2)];
    std::array::from_fn(|c| {
        ((1.0 - strength) * f32::from(primary[c]) + strength * f32::from(FOREST_COLORS[2][c]))
            .round() as u8
    })
}

pub(crate) fn atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for (page, color) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        atlas[start..start + 4].copy_from_slice(color);
    }
    atlas
}
