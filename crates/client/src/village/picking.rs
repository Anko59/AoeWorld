use super::{Client, owner};
use crate::playground::game::{map, position_at};
use aoe_core::{EntityId, ScreenPoint};
use aoe_protocol::ResourceKind;
use aoe_rendering::{SceneCamera, resource_sprite_bounds, scene_resource_frame};

/// Shared pointer target for orders and cursor feedback, units before scenery.
pub(in super::super) fn actor(
    client: &Client,
    point: ScreenPoint,
) -> Option<(EntityId, bool, bool)> {
    target(client, point, false)
}

pub(in super::super) fn unit(client: &Client, point: ScreenPoint) -> Option<EntityId> {
    target(client, point, true).map(|(id, _, _)| id)
}

fn target(client: &Client, point: ScreenPoint, units_only: bool) -> Option<(EntityId, bool, bool)> {
    if crate::playground::game::strategic::active(client) {
        return None;
    }
    let scene = map::scene::prepare(client);
    let terrain_depth = aoe_rendering::surface_depth_at(&scene.triangles, point);
    let rendered = map::scene::units(client, &scene);
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let units = client
        .units
        .values()
        .filter(|u| u.hp > 0)
        .map(|u| (u.id, false, u.player != owner(client)));
    let animals = client
        .village
        .animals
        .iter()
        .filter(|_| !units_only)
        .filter(|a| a.hp > 0 || a.food_remaining > 0)
        .map(|a| (a.id, true, false));
    units
        .chain(animals)
        .filter_map(|(id, animal, enemy)| {
            // Keep bounds on the same submitted pose as the hotspot. New
            // simulation poses may still be waiting for the GPU.
            let unit = client.rendered_frame.unit(id).or_else(|| {
                rendered
                    .binary_search_by_key(&id, |u| u.id)
                    .ok()
                    .map(|index| rendered[index])
            })?;
            let screen = map::screen_position(client, unit.position);
            let fallback = [
                screen.x - 7.0 * client.camera.zoom,
                screen.y - 30.0 * client.camera.zoom,
                screen.x + 7.0 * client.camera.zoom,
                screen.y + 2.0 * client.camera.zoom,
            ];
            let bounds = aoe_rendering::scene_unit_bounds(
                &client.art,
                unit,
                camera,
                (client.last_frame / 100.0) as usize,
            )
            .unwrap_or(fallback);
            let depth = aoe_rendering::scene_unit_depth(unit);
            (crate::unit_visuals::actor_hit(
                bounds,
                [screen.x, screen.y],
                [point.x, point.y],
                client.camera.zoom,
            ) && terrain_depth.is_none_or(|surface| surface <= depth + 1e-6))
            .then_some((depth, id, animal, enemy))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, a, e)| (id, a, e))
}

pub(super) fn enemy_building(client: &Client, point: ScreenPoint) -> Option<EntityId> {
    if resource_target(client, point).is_some() {
        return None;
    }
    let (_, id) = building_target(client, point)?;
    client
        .village
        .buildings
        .iter()
        .find(|b| b.id == id && b.player != owner(client))
        .map(|b| b.id)
}

pub(in super::super) fn click(client: &mut Client, point: ScreenPoint, unit: Option<EntityId>) {
    if let Some((id, true, _)) = actor(client, point) {
        // A ground carcass must not hide a living unit standing on it.
        let living = client
            .village
            .animals
            .iter()
            .any(|a| a.id == id && a.hp > 0);
        if living || unit.is_none() {
            client.selected.units(vec![id]);
            return;
        }
    }
    if let Some(id) = unit {
        client.selected.units(vec![id]);
        return;
    }
    if let Some((_, id, _)) = resource_target(client, point) {
        client.selected.resource(id);
    } else if let Some((_, id)) = building_target(client, point) {
        client.selected.building(id);
    } else {
        client.selected = Default::default();
    }
}

fn building_target(client: &Client, point: ScreenPoint) -> Option<(f64, EntityId)> {
    if crate::playground::game::strategic::active(client) {
        return None;
    }
    let scene = map::scene::prepare(client);
    let terrain_depth = aoe_rendering::surface_depth_at(&scene.triangles, point);
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let rendered = map::scene::units(client, &scene);
    let world = map::world_at_screen(client, point);
    client
        .village
        .buildings
        .iter()
        .filter(|b| b.hp > 0)
        .flat_map(|b| {
            // A farm's visible crop is painted into the ground mesh.
            let ground = b.kind == aoe_protocol::BuildingKind::Farm
                && world.is_some_and(|p| {
                    p[0] >= f64::from(b.footprint.min.x)
                        && p[0] < f64::from(b.footprint.max.x)
                        && p[1] >= f64::from(b.footprint.min.y)
                        && p[1] < f64::from(b.footprint.max.y)
                });
            let start = rendered.partition_point(|u| u.id < b.id);
            rendered[start..]
                .iter()
                .copied()
                .take_while(move |u| u.id == b.id)
                .map(move |u| (u, ground))
        })
        .filter_map(|(u, ground)| {
            let depth = if ground {
                world.map_or(0.0, |p| {
                    p[0] + p[1] + 2.0 * scene.height(p).unwrap_or(u.elevation_meters)
                })
            } else {
                aoe_rendering::scene_unit_depth(u)
            };
            (terrain_depth.is_none_or(|surface| surface <= depth + 1e-6)
                && (ground
                    || aoe_rendering::scene_building_hit(
                        &client.art,
                        u,
                        camera,
                        (client.last_frame / 100.0) as usize,
                        point,
                    )))
            .then_some((depth, u.id))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
}

pub(in super::super) fn double_click(client: &mut Client, unit: EntityId) {
    let Some(kind) = client.units.get(&unit).map(|u| u.kind) else {
        return;
    };
    // The HTML frame uses CSS pixels; projection uses the capped canvas backing.
    let scale = client.camera.viewport[1] / f64::from(client.canvas.client_height().max(1));
    let top = 54.0 * scale;
    let bottom = client.camera.viewport[1] - 208.0 * scale;
    let candidates = client
        .units
        .values()
        .map(|u| {
            let p = map::screen_position(client, position_at(client, u.id));
            (
                u.id,
                u.player,
                u.kind,
                p.x >= 0.0 && p.y >= top && p.x < client.camera.viewport[0] && p.y < bottom,
            )
        })
        .collect::<Vec<_>>();
    client.selected.same_kind(candidates, owner(client), kind);
}

pub(in super::super) fn double_building(client: &mut Client) {
    let Some(building) = client.village.buildings.iter().find(|b| {
        Some(b.id) == client.selected.building_id && b.player == owner(client) && b.hp > 0
    }) else {
        return;
    };
    let (kind, primary) = (building.kind, building.id);
    let scale = client.camera.viewport[1] / f64::from(client.canvas.client_height().max(1));
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let scene = map::scene::prepare(client);
    // Intersect actual sprite bounds so a partially visible roof also counts.
    let visible = map::scene::units(client, &scene)
        .into_iter()
        .filter_map(|unit| {
            let p = map::screen_position(client, unit.position);
            let bounds = aoe_rendering::scene_unit_bounds(
                &client.art,
                unit,
                camera,
                (client.last_frame / 100.0) as usize,
            )
            .unwrap_or([p.x, p.y, p.x, p.y]);
            (bounds[2] >= 0.0
                && bounds[3] >= 54.0 * scale
                && bounds[0] < client.camera.viewport[0]
                && bounds[1] < client.camera.viewport[1] - 208.0 * scale)
                .then_some(unit.id)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let candidates = client
        .village
        .buildings
        .iter()
        .map(|b| (b.id, b.player, b.kind, b.hp > 0 && visible.contains(&b.id)))
        .collect::<Vec<_>>();
    client
        .selected
        .same_buildings(candidates, owner(client), kind, primary);
}

/// The resource under the pointer, unless a nearer cliff face hides it. The
/// same frontmost-surface test as `passable` keeps gather feedback and orders
/// from reaching a resource behind an occluding cliff.
#[inline(never)]
pub(in super::super) fn resource(
    client: &Client,
    point: ScreenPoint,
) -> Option<(u64, ResourceKind)> {
    let (_, id, kind) = resource_target(client, point)?;
    Some((id, kind))
}

fn resource_target(client: &Client, point: ScreenPoint) -> Option<(f64, u64, ResourceKind)> {
    if crate::playground::game::strategic::active(client) {
        return None;
    }
    let (id, kind, world) = resource_unchecked(client, point)?;
    let scene = map::scene::prepare(client);
    let height = scene
        .height(world)
        .unwrap_or_else(|| map::elevation_at_world(client, world));
    if client.map_content_hash.is_some()
        && aoe_rendering::cliff_face_in_front(
            &scene.triangles,
            point,
            world[0] + world[1] + 2.0 * height,
        )
    {
        return None;
    }
    Some((world[0] + world[1] + height * 2.0 + 1.5, id, kind))
}

fn resource_unchecked(
    client: &Client,
    point: ScreenPoint,
) -> Option<(u64, ResourceKind, [f64; 2])> {
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let world = map::world_at_screen(client, point);
    if let Some(point) = world {
        let tile = aoe_core::TileCoord::new(point[0].floor() as i32, point[1].floor() as i32);
        let starting: Vec<_> = client.resources.starting_nodes().collect();
        if let Some(node) = client
            .terrain_chunks
            .values()
            .flat_map(|chunk| chunk.resources().map(|(node, _)| node))
            .chain(starting.iter())
            .find(|node| {
                node.tile == tile
                    && client.resources.visible(node.id)
                    && client.fog.state([tile.x, tile.y]) > 0
            })
        {
            let kind = match node.kind as u8 {
                0 => ResourceKind::Food,
                1 => ResourceKind::Wood,
                2 => ResourceKind::Gold,
                _ => ResourceKind::Stone,
            };
            return Some((
                node.id,
                kind,
                [f64::from(node.tile.x) + 0.5, f64::from(node.tile.y) + 0.5],
            ));
        }
    }
    // Use the same reviewed body rectangle as rendering: clicking a tree's crown
    // must target the tree, not the ground several tiles behind it.
    map::scene_resources(client)
        .into_iter()
        .filter(|r| r.kind <= aoe_rendering::SceneResource::CHOPPED_TREE)
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
                r.position,
            ))
        })
        .max_by(|a, b| {
            let non_tree = |kind| kind != 1 && kind != aoe_rendering::SceneResource::CHOPPED_TREE;
            non_tree(a.2)
                .cmp(&non_tree(b.2))
                .then_with(|| a.0.total_cmp(&b.0))
                .then(a.1.cmp(&b.1))
        })
        .map(|(_, id, kind, position)| {
            (
                id,
                match kind {
                    0 => ResourceKind::Food,
                    1 | aoe_rendering::SceneResource::CHOPPED_TREE => ResourceKind::Wood,
                    2 => ResourceKind::Gold,
                    _ => ResourceKind::Stone,
                },
                position,
            )
        })
}
