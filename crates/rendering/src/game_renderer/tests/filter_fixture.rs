//! Shared synthetic probes. No source artwork or performance qualification.
use crate::surface_mesh::{ProjectedSurfaceTriangle, SurfacePoint};

pub(crate) fn atlas() -> Vec<u8> {
    let mut bytes = vec![0; crate::GAME_ATLAS_BYTES];
    for pixel in bytes[..crate::GAME_ATLAS_PAGE_BYTES].chunks_exact_mut(4) {
        pixel.copy_from_slice(&[255, 0, 0, 255]);
    }
    for (base, width) in [(0, 49), (64, 49), (128, 49), (256, 49), (384, 257)] {
        for y in 0..25 {
            for x in 0..width {
                let value = if x % 2 == 0 { 0 } else { 255 };
                let alpha = if base == 128 || (base == 64 && x == 32) {
                    0
                } else if base == 256 && x % 2 != 0 {
                    128
                } else {
                    255
                };
                let index = (y * 2048 + base + x) * 4;
                bytes[index..index + 4].copy_from_slice(&[value, value, value, alpha]);
            }
        }
    }
    bytes[crate::GAME_ATLAS_PAGE_BYTES..crate::GAME_ATLAS_PAGE_BYTES + 4]
        .copy_from_slice(&[0, 255, 0, 255]);
    bytes[2 * crate::GAME_ATLAS_PAGE_BYTES..2 * crate::GAME_ATLAS_PAGE_BYTES + 4]
        .copy_from_slice(&[0, 0, 255, 255]);
    bytes
}

pub(crate) fn sprite() -> (crate::web::Sprite, crate::GameFrame) {
    let address = crate::AtlasAddress {
        page: 0,
        uv: [0.0, 0.0, 49.0 / 2048.0, 25.0 / 2048.0],
    };
    (
        crate::web::Sprite {
            position: [-0.25, 0.25],
            radius: [0.0625; 2],
            color: [1.0; 4],
            uv: address.uv,
            depths: [0.0; 4],
            terrain_blend: [[0.0; 4]; 2],
            pages: [0; 4],
        },
        crate::GameFrame {
            atlas: address,
            size: [8.0; 2],
            anchor: [0.0; 2],
        },
    )
}

pub(crate) const CASES: usize = 8;
pub(crate) fn case(index: usize) -> ([ProjectedSurfaceTriangle; 2], [u32; 2], [u8; 4]) {
    let mut face = crate::surface_mesh::floor::faces(false)[0];
    let extent = if index == 1 { 64.0 } else { 8.0 };
    let base = match index {
        4 => 64,
        5 => 128,
        6 => 384,
        7 => 256,
        _ => 0,
    };
    let screen = if index == 6 {
        // More than 1/16 pixel inside both edges; a wider frame still forces
        // right quadrant beyond the rectangle (red neighbor without clamp).
        [[16.0, 16.3], [23.7, 16.3], [16.0, 32.3]]
    } else {
        [[16.0, 16.0], [16.0 + extent, 16.0], [16.0, 16.0 + extent]]
    };
    face.points = screen.map(|[x, y]| SurfacePoint {
        world: [0.0, 0.0, 10.0],
        screen: aoe_core::ScreenPoint { x, y },
    });
    face.appearance = crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
        palette: 5,
        floor_strength: 0,
        canopy_strength: 0,
        exposure: 0,
        height_band: 0,
    }));
    face.floor_strengths = None;
    face.tint = 0;
    face.texture_blend = None;
    face.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: [
            base as f32 / 2048.0,
            0.0,
            (if index == 6 { 257.0 } else { 49.0 }) / 2048.0,
            25.0 / 2048.0,
        ],
    });
    let single = |page| crate::AtlasAddress {
        page,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    if index == 2 || index == 3 {
        face.texture_blend = Some([single(1), single(2)]);
    }
    if index == 2 {
        face.appearance = 0;
    } // Original manual three-page nearest negative control.
    if index == 3 {
        face.appearance =
            crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                palette: 5,
                floor_strength: 500,
                canopy_strength: 0,
                exposure: 0,
                height_band: 0,
            }));
    }
    let mut back = face;
    back.texture_uv = None;
    back.texture_blend = None;
    back.appearance = 0;
    back.color = [0.0, 0.0, 1.0];
    for point in &mut back.points {
        point.world = [-1.0, 0.0, 0.0];
    }
    let probe = if index == 6 { [23, 16] } else { [18, 17] };
    let mut expected = match index {
        1 => [255, 255, 255, 255],
        2 => [0, 80, 48, 255],
        3 => [64, 192, 64, 255],
        4 | 5 => [0, 0, 255, 255],
        7 => [85, 85, 85, 255],
        _ => [128, 128, 128, 255],
    };
    if face.appearance != 0 && index != 4 && index != 5 {
        let samples = if index == 3 {
            [[128, 128, 128, 255], [0, 255, 0, 255], [128, 128, 128, 255]]
        } else {
            [expected; 3]
        };
        expected = crate::surface_mesh::landscape::texel(
            samples,
            crate::surface_mesh::landscape::floor_weights(face.appearance),
            face.tint,
            face.appearance,
        );
    }
    ([back, face], probe, expected)
}
