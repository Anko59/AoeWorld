use super::*;
use aoe_rendering::{GameArt, SceneCamera, resource_sprite_bounds, scene_resource_frame};

pub(in super::super) fn scene_resources(client: &Client) -> Vec<SceneResource> {
    let visible = resident_visible_tiles(client);
    let scene = scene::prepare(client);
    let now = super::super::now();
    let starting: Vec<_> = client.resources.rendered_starting_nodes(now).collect();
    let resources = client
        .terrain_chunks
        .values()
        .filter(|chunk| heights::resident_may_be_visible(client, chunk.coordinate()))
        .flat_map(CachedChunk::resources)
        .chain(starting.iter().map(|node| {
            (
                node,
                if node.kind == aoe_map::ResourceKind::Wood {
                    aoe_map::ResourceVisualFamily::Conifer as u8
                } else {
                    aoe_map::ResourceVisualFamily::Generic as u8
                },
            )
        }))
        .filter(|(resource, _)| {
            scene_kind(resource, &client.resources, now).is_some()
                && client.fog.state([resource.tile.x, resource.tile.y]) > 0
                && resource.tile.x >= visible.min.x
                && resource.tile.x < visible.max.x
                && resource.tile.y >= visible.min.y
                && resource.tile.y < visible.max.y
        })
        .map(|(resource, visual_family)| SceneResource {
            highlight: if client.resources.visible(resource.id)
                && client.village.hovered_resource.get() == Some(resource.id)
            {
                client.hover_mode
            } else {
                0
            },
            id: resource.id,
            position: [
                f64::from(resource.tile.x) + 0.5,
                f64::from(resource.tile.y) + 0.5,
            ],
            kind: scene_kind(resource, &client.resources, now).unwrap_or(resource.kind as u8),
            visual_variant: resource.visual_variant,
            visual_family,
            elevation_meters: scene
                .height([
                    f64::from(resource.tile.x) + 0.5,
                    f64::from(resource.tile.y) + 0.5,
                ])
                .unwrap_or_else(|| elevation_at_tile(client, resource.tile.x, resource.tile.y)),
        });
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    select_visible_resources(
        resources
            .chain(decorations::scenery(&scene))
            .chain(super::super::village::rally::marker(client))
            .filter(|r| client.fog.state(r.position.map(|v| v.floor() as i32)) > 0),
        &client.art,
        camera,
    )
}

pub(super) fn select_visible_resources(
    resources: impl IntoIterator<Item = SceneResource>,
    art: &GameArt,
    camera: SceneCamera,
) -> Vec<SceneResource> {
    // Decoded residency already bounds the input (512 chunks / 128 MiB).
    // Do not turn a rendering budget into missing forest: only offscreen or
    // unavailable art is culled. GPU uploads retain their existing byte bound.
    resources
        .into_iter()
        .filter(|resource| {
            let Some(frame) = scene_resource_frame(art, *resource) else {
                return false;
            };
            resource_sprite_bounds(*resource, frame, camera)
                .is_some_and(|bounds| bounds.into_iter().all(f64::is_finite))
        })
        .collect()
}

/// Both natural and starting-kit nodes use the same authoritative stage rule.
pub(super) fn scene_kind(
    node: &ResourceNode,
    cache: &crate::resource_state::ResourceStateCache,
    now_ms: f64,
) -> Option<u8> {
    use crate::resource_state::TreeStage;
    if node.kind != aoe_map::ResourceKind::Wood {
        return cache.visible(node.id).then_some(node.kind as u8);
    }
    match cache.tree_stage(node.id, node.initial_amount, now_ms) {
        TreeStage::Full => Some(1),
        TreeStage::Chopped => Some(SceneResource::CHOPPED_TREE),
        TreeStage::Stump => Some(SceneResource::TREE_STUMP),
        TreeStage::Hidden => None,
    }
}
