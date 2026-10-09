//! Scene-only, nonblocking dressing. Never enters resource/depletion state.
use super::*;

pub(super) fn sample(
    decoration: &aoe_map::LandscapeDecoration,
    elevation_meters: f64,
) -> SceneDecoration {
    SceneDecoration {
        position: [
            f64::from(decoration.tile.x) + 0.5,
            f64::from(decoration.tile.y) + 0.5,
        ],
        family: decoration.family as u8,
        visual_variant: decoration.variant,
        orientation: decoration.orientation,
        elevation_meters,
    }
}

pub(super) fn for_scene(client: &Client, scene: &scene::PreparedScene) -> Vec<SceneDecoration> {
    let visible = resident_visible_tiles(client);
    client
        .terrain_chunks
        .values()
        .filter(|chunk| heights::resident_may_be_visible(client, chunk.coordinate()))
        .flat_map(CachedChunk::decorations)
        .filter(|decoration| {
            decoration.tile.x >= visible.min.x
                && decoration.tile.x < visible.max.x
                && decoration.tile.y >= visible.min.y
                && decoration.tile.y < visible.max.y
        })
        .map(|decoration| {
            let position = [
                f64::from(decoration.tile.x) + 0.5,
                f64::from(decoration.tile.y) + 0.5,
            ];
            sample(
                decoration,
                scene.height(position).unwrap_or_else(|| {
                    elevation_at_tile(client, decoration.tile.x, decoration.tile.y)
                }),
            )
        })
        .collect()
}
