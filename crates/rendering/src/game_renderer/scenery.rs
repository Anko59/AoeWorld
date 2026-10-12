//! Whole upright scenery frames drawn through the shared world-object path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// Reviewed faces falling toward the camera. The opposite side mirrors the
/// complete image and hotspot, never its geometry or individual source rows.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<(GameFrame, bool)> {
    let faces = &art.scenery.cliff_faces;
    if faces.len() != 25 {
        return None;
    }
    let (choices, mirror): (&[u8], bool) = match resource.visual_family {
        0 => (&[4, 5, 15, 17, 19], false),
        1 => (&[1, 2, 16], false),
        7 => (&[1, 2, 16], true),
        2 => (&[3, 18, 24], false),
        6 => (&[3, 18, 24], true),
        _ => return None,
    };
    let mut frame =
        faces[usize::from(choices[usize::from(resource.visual_variant) % choices.len()])];
    if mirror {
        frame.anchor[0] = frame.size[0] - frame.anchor[0];
    }
    Some((frame, mirror))
}
