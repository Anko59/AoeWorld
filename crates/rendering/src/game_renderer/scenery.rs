//! Whole upright scenery frames drawn through the shared world-object path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// A scenery piece selected from its approved source group.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<(GameFrame, bool)> {
    match resource.kind {
        SceneResource::CLIFF => cliff_face(art, resource),
        SceneResource::ROCK => rock(art, resource).map(|frame| (frame, false)),
        _ => None,
    }
}

fn rock(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    let rocks = &art.scenery.rocks;
    let mut frame = *rocks.get(usize::from(resource.visual_variant) % rocks.len().max(1))?;
    let scale = 0.45 + f32::from((resource.visual_variant >> 3) & 3) * 0.15;
    frame.size = frame.size.map(|value| value * scale);
    frame.anchor = frame.anchor.map(|value| value * scale);
    Some(frame)
}

/// Reviewed faces falling toward the camera. The upper-right ridge faces
/// southwest, the upper-left ridge southeast; front corners join the two.
/// Native opposing faces retain the authored sun direction and raw hotspot.
fn cliff_face(art: &GameArt, resource: SceneResource) -> Option<(GameFrame, bool)> {
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
