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
