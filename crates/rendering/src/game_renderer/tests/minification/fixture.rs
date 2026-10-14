//! Synthetic atlas and screen geometry shared by the minification pixel
//! tests. No private source pixels: every texel is generated here.
use super::*;

pub(crate) const VIEWPORT: [f64; 2] = [128.0; 2];
pub(crate) const PAGE: u32 = 2;
pub(crate) const BACKGROUND: [u8; 4] = [41, 74, 36, 255];
/// One sixteenth of a screen pixel: shifts every sample point off atlas texel
/// boundaries so nearest picks agree across backends, without changing which
/// texels a pixel's footprint covers.
pub(crate) const PHASE: f64 = 1.0 / 16.0;
/// Halo-phase shift: at two texels per screen pixel a column's tap pair
/// straddles the green/transparent boundary in both orientations, while every
/// pixel centre stays strictly inside the frame rectangle (away from the
/// neighbouring atlas entry). Dyadic, so f32 rounding cannot move a tap.
pub(crate) const HALO_PHASE: f64 = 0.375;

pub(crate) const BLACK: [u8; 4] = [0, 0, 0, 255];
pub(crate) const GREEN: [u8; 4] = [0, 255, 0, 255];
pub(crate) const RED: [u8; 4] = [255, 0, 0, 255];
pub(crate) const BLUE: [u8; 4] = [0, 0, 255, 255];
/// The classic halo trap: red colour under zero alpha.
pub(crate) const HIDDEN_RED: [u8; 4] = [255, 0, 0, 0];

/// Frame rectangles on page 2, sixteen texels wide and directly adjacent, so
/// a tap escaping a frame's own rectangle would sample its neighbour.
pub(crate) const CHECKER: [f32; 4] = [0.0, 64.0, 16.0, 16.0];
pub(crate) const HALO: [f32; 4] = [32.0, 64.0, 16.0, 16.0];

pub(crate) fn atlas() -> Vec<u8> {
    let mut bytes = vec![0; crate::GAME_ATLAS_BYTES];
    let page = crate::GAME_ATLAS_PAGE_BYTES * PAGE as usize;
    let mut put = |x: usize, y: usize, color: [u8; 4]| {
        let offset = page + (y * crate::GAME_ATLAS_SIDE as usize + x) * 4;
        bytes[offset..offset + 4].copy_from_slice(&color);
    };
    for y in 64..80 {
        for x in 0..16 {
            put(x, y, checker([x as u32, y as u32 - 64]));
        }
        for x in 16..32 {
            put(x, y, RED);
        }
        for x in 32..48 {
            put(x, y, if x < 40 { GREEN } else { HIDDEN_RED });
        }
        for x in 48..64 {
            put(x, y, BLUE);
        }
    }
    bytes
}

/// Two-by-two block high-contrast checker over the 16-texel checker frame.
pub(crate) fn checker(local: [u32; 2]) -> [u8; 4] {
    if (local[0] / 2 + local[1] / 2) % 2 == 0 {
        GREEN
    } else {
        BLACK
    }
}

/// Halo-frame texel: opaque green left half, transparent red right half.
pub(crate) fn halo(local_x: u32) -> [u8; 4] {
    if local_x < 8 { GREEN } else { HIDDEN_RED }
}

pub(crate) struct Placed {
    pub(crate) sprite: Sprite,
    pub(crate) frame: GameFrame,
    pub(crate) left_top: [f64; 2],
}

/// Places `size` screen pixels of `rect` with its top-left at `left_top`
/// (plus `phase`); the Canvas renderer derives the floor/ceil bounds from
/// the frame size itself. `flipped` mirrors the frame's own x span
/// exactly as flipped unit sprites do.
pub(crate) fn placed(
    rect: [f32; 4],
    left_top: [f64; 2],
    size: [f64; 2],
    flipped: bool,
    phase: f64,
) -> Placed {
    let side = f64::from(crate::GAME_ATLAS_SIDE);
    let left_top = [left_top[0] + phase, left_top[1] + phase];
    let span = [f64::from(rect[2]), f64::from(rect[3])];
    let uv = [
        (f64::from(rect[0]) + if flipped { span[0] } else { 0.0 }) / side,
        f64::from(rect[1]) / side,
        (if flipped { -span[0] } else { span[0] }) / side,
        span[1] / side,
    ];
    let sprite = Sprite {
        position: [
            ((left_top[0] + size[0] / 2.0) / VIEWPORT[0] * 2.0 - 1.0) as f32,
            (1.0 - (left_top[1] + size[1] / 2.0) / VIEWPORT[1] * 2.0) as f32,
        ],
        radius: [
            (size[0] / VIEWPORT[0]) as f32,
            (size[1] / VIEWPORT[1]) as f32,
        ],
        color: [1.0; 4],
        uv: uv.map(|value| value as f32),
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [PAGE, 0, 0, 0],
    };
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: PAGE,
            uv: sprite.uv,
        },
        size: size.map(|value| value as f32),
        anchor: [0.0; 2],
    };
    Placed {
        sprite,
        frame,
        left_top,
    }
}

/// The bounded four-tap premultiplied box average the minified sprite path
/// computes, in atlas integers. `None` when the summed alpha is zero: the
/// nearest centre then wins, exactly as on the GPU.
pub(crate) fn box_average(taps: [[u8; 4]; 4]) -> Option<[u8; 4]> {
    let mut sums = [0_u32; 4];
    for tap in taps {
        for channel in 0..3 {
            sums[channel] += u32::from(tap[channel]) * u32::from(tap[3]);
        }
        sums[3] += u32::from(tap[3]);
    }
    if sums[3] == 0 {
        return None;
    }
    Some(std::array::from_fn(|channel| {
        if channel < 3 {
            (f64::from(sums[channel]) / f64::from(sums[3]) + 0.5).floor() as u8
        } else {
            (f64::from(sums[3]) / 4.0 + 0.5).floor() as u8
        }
    }))
}

/// Straight-alpha composite over an opaque target, mirroring the shared
/// write kernel: `(src*a + dst*(255-a) + 127) / 255` per colour channel.
pub(crate) fn over(source: [u8; 4], target: [u8; 4]) -> [u8; 4] {
    if source[3] == u8::MAX {
        return source;
    }
    if source[3] == 0 {
        return target;
    }
    let source_alpha = u32::from(source[3]);
    let target_alpha = 255 - source_alpha;
    std::array::from_fn(|channel| {
        if channel < 3 {
            ((u32::from(source[channel]) * source_alpha
                + u32::from(target[channel]) * target_alpha
                + 127)
                / 255) as u8
        } else {
            255
        }
    })
}

pub(crate) fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = (y as usize * VIEWPORT[0] as usize + x as usize) * 4;
    [
        image[offset],
        image[offset + 1],
        image[offset + 2],
        image[offset + 3],
    ]
}
