//! Frames of scenery drawn through the resource sprite path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// Frame of a cliff piece and whether it is mirrored.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<(GameFrame, bool)> {
    cliff_band(art, resource)
}

/// Ground art is authored for 97-pixel tiles and drawn on 128-pixel tiles, so
/// scenery that lies on the ground is enlarged like the ground under it.
const GROUND_SCALE: f32 = (aoe_core::ISO_TILE_WIDTH / 97.0) as f32;
/// Source pixels per unit of `x + y` ground distance.
const DIAGONAL_PIXELS: f32 = (aoe_core::ISO_TILE_HEIGHT / 2.0) as f32 / GROUND_SCALE;
/// Rows a band reaches back under its neighbour so slopes leave no gap.
const BAND_OVERLAP_ROWS: f32 = 4.0;

/// Sub-frame of one cliff band and whether it is mirrored. Faces that fall to
/// the southeast or east are the mirrored southwest and west faces.
fn cliff_band(art: &GameArt, resource: SceneResource) -> Option<(GameFrame, bool)> {
    let faces = &art.scenery.cliff_faces;
    if faces.len() != 25 {
        return None;
    }
    let (choices, mirror): (&[u8], bool) = match resource.visual_family & 7 {
        0 => (&[4, 5, 15, 19], false),
        1 => (&[1, 2, 16], false),
        7 => (&[1, 2, 16], true),
        2 => (&[3, 24, 18], false),
        6 => (&[3, 24, 18], true),
        _ => return None,
    };
    let band = (resource.visual_family >> 3) & 7;
    let mut frame =
        faces[usize::from(choices[usize::from(resource.visual_variant) % choices.len()])];
    let [width, height] = frame.size;
    let near =
        frame.anchor[1] + (SceneResource::cliff_band_offset(band) * 2.0) as f32 * DIAGONAL_PIXELS;
    let far = near - SceneResource::CLIFF_BAND_SPAN as f32 * DIAGONAL_PIXELS;
    let first = if band == 0 {
        0.0
    } else {
        (far - BAND_OVERLAP_ROWS).floor().clamp(0.0, height)
    };
    let last = if band + 1 == SceneResource::CLIFF_BANDS {
        height
    } else {
        near.ceil().clamp(0.0, height)
    };
    if last <= first || height <= 0.0 {
        return None;
    }
    frame.atlas.uv[1] += frame.atlas.uv[3] * first / height;
    frame.atlas.uv[3] *= (last - first) / height;
    frame.size = [width * GROUND_SCALE, (last - first) * GROUND_SCALE];
    frame.anchor = [
        GROUND_SCALE
            * if mirror {
                width - frame.anchor[0]
            } else {
                frame.anchor[0]
            },
        (near - first) * GROUND_SCALE,
    ];
    Some((frame, mirror))
}
