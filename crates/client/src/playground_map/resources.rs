use super::*;
use aoe_rendering::{GameArt, SceneCamera, resource_sprite_bounds};

const RESOURCE_SELECTION_AXIS: usize = 32;

pub(in super::super) fn scene_resources(client: &Client) -> Vec<SceneResource> {
    let visible = resident_visible_tiles(client);
    let resources = client
        .terrain_chunks
        .values()
        .filter(|chunk| heights::resident_may_be_visible(client, (chunk.x, chunk.y)))
        .flat_map(|chunk| chunk.resources.iter())
        .filter(|resource| {
            client.resources.visible(resource.id)
                && resource.tile.x >= visible.min.x
                && resource.tile.x < visible.max.x
                && resource.tile.y >= visible.min.y
                && resource.tile.y < visible.max.y
        })
        .map(|resource| SceneResource {
            id: resource.id,
            position: [
                f64::from(resource.tile.x) + 0.5,
                f64::from(resource.tile.y) + 0.5,
            ],
            kind: resource.kind as u8,
            visual_variant: resource.visual_variant,
            elevation_meters: elevation_at_tile(client, resource.tile.x, resource.tile.y),
        });
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    select_visible_resources(resources, &client.art, camera)
}

pub(super) fn select_visible_resources(
    resources: impl IntoIterator<Item = SceneResource>,
    art: &GameArt,
    camera: SceneCamera,
) -> Vec<SceneResource> {
    let mut under_budget = Vec::with_capacity(MAX_VISIBLE_RESOURCE_SPRITES + 1);
    let mut cells = vec![None::<(SceneResource, u64)>; RESOURCE_SELECTION_AXIS.pow(2)];
    let mut overflow = false;
    for resource in resources {
        let Some(frames) = art.resources.get(usize::from(resource.kind)) else {
            continue;
        };
        if frames.is_empty() {
            continue;
        }
        let frame = frames[usize::from(resource.visual_variant) % frames.len()];
        let Some([left, top, right, bottom]) = resource_sprite_bounds(resource, frame, camera)
        else {
            continue;
        };
        if ![left, top, right, bottom].into_iter().all(f64::is_finite) {
            continue;
        }
        let sample_x = (left.max(0.0) + right.min(camera.viewport[0])) * 0.5;
        let sample_y = (top.max(0.0) + bottom.min(camera.viewport[1])) * 0.5;
        let x_cell = ((sample_x / camera.viewport[0] * RESOURCE_SELECTION_AXIS as f64).floor()
            as usize)
            .min(RESOURCE_SELECTION_AXIS - 1);
        let y_cell = ((sample_y / camera.viewport[1] * RESOURCE_SELECTION_AXIS as f64).floor()
            as usize)
            .min(RESOURCE_SELECTION_AXIS - 1);
        let index = y_cell * RESOURCE_SELECTION_AXIS + x_cell;
        let rank = stable_resource_rank(resource.id);
        let replace = cells[index].is_none_or(|(current, current_rank)| {
            rank < current_rank || (rank == current_rank && resource.id < current.id)
        });
        if replace {
            cells[index] = Some((resource, rank));
        }
        if under_budget.len() <= MAX_VISIBLE_RESOURCE_SPRITES {
            under_budget.push(resource);
            if under_budget.len() > MAX_VISIBLE_RESOURCE_SPRITES {
                overflow = true;
            }
        }
    }
    if !overflow {
        under_budget.sort_by_key(|resource| resource.id);
        return under_budget;
    }
    let mut selected = cells
        .into_iter()
        .flatten()
        .map(|(resource, _)| resource)
        .collect::<Vec<_>>();
    selected.sort_by_key(|resource| resource.id);
    selected
}

fn stable_resource_rank(id: u64) -> u64 {
    let mut value = id.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
