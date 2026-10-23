//! Whole upright scenery frames drawn through the shared world-object path.
use crate::game_renderer::SceneResource;
use crate::{GameArt, GameFrame};

/// A scenery piece selected from its approved source group.
pub(super) fn frame(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    match resource.kind {
        SceneResource::RALLY_FLAG => art
            .scenery
            .rally_flags
            .get(usize::from(resource.visual_variant) % 10)
            .copied(),
        // Oak, pine and palm all reference graphic 1699 / SLP 1252 in the trial.
        SceneResource::CHOPPED_TREE => art
            .scenery
            .logs
            .get(usize::from(resource.visual_variant) % 2)
            .copied(),
        SceneResource::TREE_STUMP => art
            .scenery
            .stumps
            .get(usize::from(resource.visual_variant) % 3)
            .copied(),
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
    // Low loose rocks are distinct from the tall gatherable ore piles.
    const PILES: [usize; 3] = [0, 2, 4];
    Some(rocks[PILES[usize::from(resource.visual_variant) % PILES.len()]])
}

fn dressing(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    // Native variants, no mirroring: preserve the original light direction.
    let variant = usize::from(resource.visual_variant) + usize::from(resource.visual_family);
    let scenery = &art.scenery;
    match resource.kind {
        SceneResource::RALLY_FLAG => art
            .scenery
            .rally_flags
            .get(usize::from(resource.visual_variant) % 10)
            .copied(),
        SceneResource::SHRUB if scenery.shrubs.len() == 6 => Some(scenery.shrubs[3 + variant % 3]),
        // The pack's yellow patches read as ore, and its tan clumps as stumps.
        // Keep meadow dressing out of the resource silhouette vocabulary.
        SceneResource::GRASS => None,
        SceneResource::DEADWOOD if scenery.stumps.len() == 3 && scenery.logs.len() == 2 => {
            // Decoration uses cut stumps and fallen logs, never upright snags
            // whose silhouettes look like harvestable trees.
            const WOOD: [usize; 5] = [0, 1, 2, 3, 4];
            let index = WOOD[variant % WOOD.len()];
            Some(match index {
                0..=2 => scenery.stumps[index],
                3..=4 => scenery.logs[index - 3],
                _ => return None,
            })
        }
        _ => None,
    }
}
