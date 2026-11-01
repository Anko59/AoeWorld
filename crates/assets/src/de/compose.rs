//! SLD frame to colour, player-ramp and shadow planes.
use super::FrameInfo;
use crate::{
    pack::atlas::Planes,
    sld::{Frame, Layer},
};

/// Shadow intensity below which a texel is left clear.
const SHADOW_FLOOR: u8 = 16;

/// The cropped rectangle of a frame: the union of the colour and shadow layers.
fn bounds(frame: &Frame) -> Option<(u16, u16, u16, u16)> {
    [&frame.main, &frame.shadow]
        .into_iter()
        .flatten()
        .filter(|layer| layer.width > 0 && layer.height > 0)
        .map(|layer| {
            (
                layer.x,
                layer.y,
                layer.x + layer.width,
                layer.y + layer.height,
            )
        })
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

pub(super) fn info(frame: &Frame) -> FrameInfo {
    let (x, y, right, bottom) = bounds(frame).unwrap_or((0, 0, 1, 1));
    FrameInfo {
        width: right - x,
        height: bottom - y,
        anchor: (
            i32::from(frame.anchor_x) - i32::from(x),
            i32::from(frame.anchor_y) - i32::from(y),
        ),
        canvas: (frame.width, frame.height),
        body: frame
            .main
            .as_ref()
            .map_or((0, 0), |layer| (layer.width, layer.height)),
    }
}

fn luma(pixel: [u8; 4]) -> u32 {
    (54 * u32::from(pixel[0]) + 183 * u32::from(pixel[1]) + 19 * u32::from(pixel[2])) / 256
}

/// Copies `layer` into the planes through `write(plane index, layer pixel)`.
fn each(layer: &Layer, origin: (u16, u16), width: usize, mut write: impl FnMut(usize, [u8; 4])) {
    let (dx, dy) = (layer.x - origin.0, layer.y - origin.1);
    for row in 0..usize::from(layer.height) {
        for col in 0..usize::from(layer.width) {
            let at = (usize::from(dy) + row) * width + usize::from(dx) + col;
            write(at, layer.pixels[row * usize::from(layer.width) + col]);
        }
    }
}

/// Builds the planes for one frame, cropped to the drawn layers with the
/// anchor moved by the crop. A frame without layers becomes one clear texel.
pub(super) fn planes(frame: &Frame) -> Planes {
    let anchor = (i32::from(frame.anchor_x), i32::from(frame.anchor_y));
    let Some((x0, y0, x1, y1)) = bounds(frame) else {
        return Planes::blank(1, 1, anchor.0, anchor.1);
    };
    let (width, height) = (x1 - x0, y1 - y0);
    let mut planes = Planes::blank(
        width,
        height,
        anchor.0 - i32::from(x0),
        anchor.1 - i32::from(y0),
    );
    let stride = usize::from(width);
    if let Some(main) = &frame.main {
        each(main, (x0, y0), stride, |at, pixel| {
            if pixel[3] > 0 {
                planes.color[at * 4..at * 4 + 4]
                    .copy_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        });
        // The player mask shares the colour layer's rectangle. The ramp value
        // keeps the colour layer's own relief so team-coloured roofs stay shaded;
        // alpha retains DE's blend weight, including values below 128.
        if let Some(mask) = &frame.player {
            each(mask, (x0, y0), stride, |at, pixel| {
                let base = &planes.color[at * 4..at * 4 + 4];
                if pixel[0] > 0 && base[3] > 0 {
                    let ramp = (luma([base[0], base[1], base[2], 255]) * 7 + 127) / 255;
                    planes.player[at * 4..at * 4 + 4].copy_from_slice(&[
                        ramp.min(7) as u8,
                        0,
                        0,
                        pixel[0],
                    ]);
                }
            });
        }
    }
    if let Some(shadow) = &frame.shadow {
        each(shadow, (x0, y0), stride, |at, pixel| {
            if pixel[0] >= SHADOW_FLOOR {
                let alpha = (u16::from(pixel[0]) * 128 / 255) as u8;
                planes.shadow[at * 4..at * 4 + 4].copy_from_slice(&[0, 0, 0, alpha]);
            }
        });
    }
    planes
}

#[path = "compose/tests.rs"]
#[cfg(test)]
mod tests;
