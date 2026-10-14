//! Pins dev's nearest terrain-quad pixels independently of object filtering.
use super::*;

pub(crate) const PROBES: [[u32; 2]; 5] = [[64, 64], [60, 60], [70, 65], [20, 30], [94, 90]];

pub(crate) type Quad = (Sprite, GameFrame, f64, u64);

pub(crate) fn scene(zoom: f64) -> (SceneCamera, Vec<Quad>, Vec<u8>) {
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.0, 64.0 / 2048.0, 96.0 / 2048.0, 48.0 / 2048.0],
        },
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    };
    let art = GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame],
        terrain: std::array::from_fn(|_| vec![frame]),
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
    };
    let camera = SceneCamera {
        center: [0.5078125, 0.5],
        zoom,
        viewport: fixture::VIEWPORT,
        focus_elevation_meters: 0.0,
    };
    let mut atlas = fixture::atlas();
    for y in 0..48_usize {
        for x in 0..96_usize {
            let inside = ((x as f64 + 0.5 - 48.0) / 48.0).abs()
                + ((y as f64 + 0.5 - 24.0) / 24.0).abs()
                <= 1.0;
            let color = if !inside {
                [0; 4]
            } else if x % 2 == 0 {
                fixture::GREEN
            } else {
                fixture::BLACK
            };
            let offset = ((y + 64) * 2048 + x) * 4;
            atlas[offset..offset + 4].copy_from_slice(&color);
        }
    }
    let quads = world_sprites::world_sprite_frames(&art, &[], &[], &[], camera, 0);
    assert!(!quads.is_empty());
    (camera, quads, atlas)
}

/// Dev's quad coverage and nearest lookup, with no minification kernel.
/// Canvas visits floor/ceil bounds; GPU visits pixel centres inside the quad.
pub(crate) fn nearest_image(quads: &[Quad], atlas: &[u8], gpu: bool) -> Vec<u8> {
    let mut image = fixture::BACKGROUND.repeat(128 * 128);
    for (sprite, frame, _, _) in quads {
        let origin = [
            (f64::from(sprite.position[0]) + 1.0) * 64.0 - f64::from(frame.size[0]) / 2.0,
            (1.0 - f64::from(sprite.position[1])) * 64.0 - f64::from(frame.size[1]) / 2.0,
        ];
        let lo = origin.map(|v| v.floor().clamp(0.0, 128.0) as u32);
        let hi = [0, 1].map(|a| {
            (origin[a] + f64::from(frame.size[a]))
                .ceil()
                .clamp(0.0, 128.0) as u32
        });
        for y in lo[1]..hi[1] {
            for x in lo[0]..hi[0] {
                let local = [0, 1]
                    .map(|a| (f64::from([x, y][a]) + 0.5 - origin[a]) / f64::from(frame.size[a]));
                if gpu && local.iter().any(|v| !(0.0..1.0).contains(v)) {
                    continue;
                }
                let texel = [0, 1].map(|a| {
                    ((f64::from(sprite.uv[a]) + local[a] * f64::from(sprite.uv[a + 2])) * 2048.0)
                        .floor()
                        .clamp(0.0, 2047.0) as usize
                });
                let source = (texel[1] * 2048 + texel[0]) * 4;
                if atlas[source + 3] == 0 {
                    continue;
                }
                let target = (y as usize * 128 + x as usize) * 4;
                image[target..target + 4].copy_from_slice(&atlas[source..source + 4]);
            }
        }
    }
    image
}
