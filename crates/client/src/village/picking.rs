use super::{Client, owner};
use crate::playground::game::{map, position_at};
use aoe_core::{EntityId, ScreenPoint};
use aoe_protocol::ResourceKind;
use aoe_rendering::{SceneCamera, resource_sprite_bounds, scene_resource_frame};

pub(in super::super) fn click(client: &mut Client, point: ScreenPoint, unit: Option<EntityId>) {
    if let Some(id) = unit {
        client.selected.units(vec![id]);
        return;
    }
    if let Some(world) = map::world_at_screen(client, point)
        && let Some(building) = client.village.buildings.iter().find(|b| {
            world[0] >= f64::from(b.footprint.min.x)
                && world[0] < f64::from(b.footprint.max.x)
                && world[1] >= f64::from(b.footprint.min.y)
                && world[1] < f64::from(b.footprint.max.y)
        })
    {
        client.selected.building(building.id);
    } else {
        client.selected = Default::default();
    }
}

pub(in super::super) fn double_click(client: &mut Client, unit: EntityId) {
    let Some(kind) = client.units.get(&unit).map(|u| u.kind) else {
        return;
    };
    let candidates = client
        .units
        .values()
        .map(|u| {
            let p = map::screen_position(client, position_at(client, u.id));
            (
                u.id,
                u.player,
                u.kind,
                p.x >= 0.0
                    && p.y >= 54.0
                    && p.x < client.camera.viewport[0]
                    && p.y < client.camera.viewport[1] - 208.0,
            )
        })
        .collect::<Vec<_>>();
    client.selected.same_kind(candidates, owner(client), kind);
}

pub(in super::super) fn resource(
    client: &Client,
    point: ScreenPoint,
) -> Option<(u64, ResourceKind)> {
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let world = map::world_at_screen(client, point);
    if let Some(point) = world {
        let tile = aoe_core::TileCoord::new(point[0].floor() as i32, point[1].floor() as i32);
        if let Some((node, _)) = client
            .terrain_chunks
            .values()
            .flat_map(|chunk| chunk.resources())
            .find(|(node, _)| node.tile == tile && client.resources.visible(node.id))
        {
            let kind = match node.kind as u8 {
                0 => ResourceKind::Food,
                1 => ResourceKind::Wood,
                2 => ResourceKind::Gold,
                _ => ResourceKind::Stone,
            };
            return Some((node.id, kind));
        }
    }
    // Use the same reviewed body rectangle as rendering: clicking a tree's crown
    // must target the tree, not the ground several tiles behind it.
    map::scene_resources(client)
        .into_iter()
        .filter(|r| r.kind < 4)
        .filter_map(|r| {
            let bounds = scene_resource_frame(&client.art, r)
                .and_then(|f| resource_sprite_bounds(r, f, camera));
            let body = bounds.is_some_and(|[l, t, right, b]| {
                point.x >= l && point.x <= right && point.y >= t && point.y <= b
            });
            let tile = world.is_some_and(|p| {
                p[0].floor() == r.position[0].floor() && p[1].floor() == r.position[1].floor()
            });
            (body || tile).then_some((
                r.position[0] + r.position[1] + r.elevation_meters * 2.0,
                r.id,
                r.kind,
            ))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, kind)| {
            (
                id,
                match kind {
                    0 => ResourceKind::Food,
                    1 => ResourceKind::Wood,
                    2 => ResourceKind::Gold,
                    _ => ResourceKind::Stone,
                },
            )
        })
}
