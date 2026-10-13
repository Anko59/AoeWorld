//! Whole upright scenery frames drawn through the shared world-object path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// A scenery piece selected from its approved source group.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    match resource.kind {
        SceneResource::ROCK => rock(art, resource),
        _ => None,
    }
}

fn rock(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    let rocks = &art.scenery.rocks;
    if rocks.len() != 7 {
        return None;
    }
    // Three quarters are the broad, tall piles; keep every native shape.
    const PILES: [usize; 16] = [5, 3, 6, 5, 1, 5, 6, 3, 2, 5, 6, 3, 4, 5, 6, 0];
    Some(rocks[PILES[usize::from(resource.visual_variant) % PILES.len()]])
}
