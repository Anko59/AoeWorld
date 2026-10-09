//! Synthetic family art/pixels only; no private source pixels or packing proof.
use super::*;

pub(crate) const CLEAR: [u8; 4] = [41, 74, 36, 255];
pub(crate) const PROBES: [[u32; 2]; 4] = [[60, 40], [68, 40], [66, 66], [74, 66]];

pub(crate) fn camera() -> SceneCamera {
    SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    }
}
pub(crate) fn resource(family: u8, variant: u8) -> SceneResource {
    SceneResource {
        id: 7,
        position: [0.5; 2],
        kind: 1,
        visual_variant: variant,
        visual_family: family,
        elevation_meters: 0.0,
    }
}
pub(crate) fn frame(x: usize) -> GameFrame {
    GameFrame {
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [x as f32 / 2048.0, 0.0, 16.0 / 2048.0, 32.0 / 2048.0],
        },
        size: [16.0, 32.0],
        anchor: [8.0, 32.0],
    }
}
pub(crate) fn art() -> GameArt {
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: [None; 7],
        terrain_world: None,
        resources: std::array::from_fn(|kind| {
            if kind == 1 {
                (0..14).map(|i| frame(640 + i * 16)).collect()
            } else {
                vec![frame(640)]
            }
        }),
        tree_shadows: (0..14).map(|i| frame(896 + i * 16)).collect(),
        tree_families: [
            (0..9).map(|i| frame(i * 16)).collect(),
            (0..13).map(|i| frame(192 + i * 16)).collect(),
        ],
    }
}
pub(crate) fn atlas() -> Vec<u8> {
    let mut bytes = vec![0; crate::GAME_ATLAS_BYTES];
    for (start, count, color) in [
        (0, 9, [0, 255, 0, 255]),
        (192, 13, [255, 0, 255, 255]),
        (640, 14, [255, 0, 0, 255]),
        (896, 14, [0, 0, 255, 255]),
    ] {
        for i in 0..count {
            for y in 0..32 {
                for x in 0..8 {
                    let offset =
                        crate::GAME_ATLAS_PAGE_BYTES * 2 + (y * 2048 + start + i * 16 + x) * 4;
                    bytes[offset..offset + 4].copy_from_slice(&color);
                }
            }
        }
    }
    bytes
}
pub(crate) fn drawn(
    art: &GameArt,
    resources: &[SceneResource],
) -> Vec<(Sprite, GameFrame, f64, u64)> {
    world_sprite_frames(art, &[], resources, &[], camera(), 0)
}
pub(crate) fn expected(family: u8) -> [[u8; 4]; 4] {
    [
        if family == 2 {
            [0, 255, 0, 255]
        } else {
            [255, 0, 255, 255]
        },
        CLEAR,
        [33, 59, 29, 255],
        CLEAR,
    ]
}
