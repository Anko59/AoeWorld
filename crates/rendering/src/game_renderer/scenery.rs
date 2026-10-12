//! Whole upright scenery frames drawn through the shared world-object path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// Reviewed faces falling toward the camera. The upper-right ridge faces
/// southwest, the upper-left ridge southeast; front corners join the two.
/// Native opposing faces retain the authored sun direction and raw hotspot.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<(GameFrame, bool)> {
    let faces = &art.scenery.cliff_faces;
    if faces.len() != 25 {
        return None;
    }
    let choices: &[u8] = match resource.visual_family {
        0 => &[3, 15, 17, 19, 24],
        1 | 2 => &[1, 2, 18],
        6 | 7 => &[4, 5, 16],
        _ => return None,
    };
    let frame = faces[usize::from(choices[usize::from(resource.visual_variant) % choices.len()])];
    Some((frame, false))
}
