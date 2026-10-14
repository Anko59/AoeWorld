//! Whole upright scenery frames drawn through the shared world-object path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// A scenery piece selected from its approved source group.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    match resource.kind {
        SceneResource::ROCK => rock(art, resource),
        SceneResource::SHRUB | SceneResource::GRASS | SceneResource::DEADWOOD => {
            dressing(art, resource)
        }
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

fn dressing(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    // Native variants, no mirroring: preserve the original light direction.
    let variant = usize::from(resource.visual_variant) + usize::from(resource.visual_family);
    let scenery = &art.scenery;
    match resource.kind {
        SceneResource::SHRUB if scenery.shrubs.len() == 6 => Some(scenery.shrubs[3 + variant % 3]),
        SceneResource::GRASS if scenery.grass.len() == 7 && scenery.reeds.len() == 2 => {
            const PATCHES: [usize; 15] = [7, 8, 0, 7, 1, 8, 2, 7, 3, 8, 4, 7, 5, 8, 6];
            let index = PATCHES[variant % PATCHES.len()];
            Some(if index < 7 {
                scenery.grass[index]
            } else {
                scenery.reeds[index - 7]
            })
        }
        SceneResource::DEADWOOD
            if scenery.stumps.len() == 3 && scenery.logs.len() == 2 && scenery.snags.len() == 1 =>
        {
            const WOOD: [usize; 12] = [3, 4, 3, 0, 4, 1, 3, 4, 2, 3, 4, 5];
            let index = WOOD[variant % WOOD.len()];
            Some(match index {
                0..=2 => scenery.stumps[index],
                3..=4 => scenery.logs[index - 3],
                _ => scenery.snags[0],
            })
        }
        _ => None,
    }
}
