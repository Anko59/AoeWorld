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

/// Scenery drawn through the resource sprite path: every cliff piece becomes
/// its bands. All bands stand at the height of the piece centre; a band whose
/// ground row rises above that height is lifted in depth so the ground never
/// covers it.
pub(super) fn scenery<'a>(
    client: &'a Client,
    scene: &'a scene::PreparedScene,
) -> impl Iterator<Item = SceneResource> + 'a {
    let height = move |position: [f64; 2]| {
        scene
            .height(position)
            .unwrap_or_else(|| elevation_at_world(client, position))
    };
    scene
        .decorations
        .iter()
        .filter(|decoration| decoration.family == aoe_map::DecorationFamily::Cliff as u8)
        .flat_map(move |decoration| {
            let center = decoration.position;
            let tile = [center[0].floor() as i32, center[1].floor() as i32];
            let elevation_meters = height(center);
            (0..SceneResource::CLIFF_BANDS).map(move |band| {
                let offset = SceneResource::cliff_band_offset(band);
                let position = [center[0] + offset, center[1] + offset];
                // Highest ground along the band: its middle and both ends.
                let rise = [-1.0, 0.0, 1.0]
                    .into_iter()
                    .map(|side| height([position[0] + side, position[1] - side]))
                    .fold(elevation_meters, f64::max)
                    - elevation_meters;
                let lift = (2.0 * rise / SceneResource::CLIFF_DEPTH_LIFT).ceil() as u8;
                SceneResource {
                    // Odd identifiers never collide with resource nodes.
                    id: ((tile[1] as u64) << 19) | ((tile[0] as u64) << 1) | 1,
                    position,
                    kind: SceneResource::CLIFF,
                    visual_variant: decoration.visual_variant,
                    visual_family: SceneResource::cliff_family(decoration.orientation, band, lift),
                    elevation_meters,
                }
            })
        })
}
