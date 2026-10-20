//! Bounded HUD extras: authoritative resource selection and strategic markers.
use super::{Client, building_center, owner};
use crate::playground::game::map;
use crate::resource_state::ResourceStateCache;
use aoe_core::{ScreenPoint, TileCoord};
use aoe_protocol::UnitKind;
pub(super) fn values(client: &Client) -> Vec<f64> {
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    let owner = owner(client);
    let mut values = vec![0.0; 9];
    values[0] = f64::from(u8::from(
        client
            .socket
            .as_ref()
            .is_some_and(|s| s.ready_state() == web_sys::WebSocket::OPEN),
    ));
    // Header: connection, selected kind/remaining, cursor, highlight center,
    // actor count, resource count, highlight width. Own actors are world triples.
    values[1] = -1.0;
    for u in client.units.values() {
        if u.player == owner {
            values[6] += 1.0;
            marker(&mut values, 0.0, u.position.as_tiles());
        }
    }
    for b in &client.village.buildings {
        if b.player == owner {
            values[6] += 1.0;
            marker(&mut values, 1.0, building_center(b));
        }
    }

    let focus = client.selected.units.iter().find_map(|id| {
        client
            .units
            .get(id)
            .filter(|u| u.kind == UnitKind::Villager)
            .map(|u| u.position.as_tiles())
    });
    let villager = focus.is_some();
    let hover = if villager && !client.village.move_cursor {
        client
            .pointer
            .and_then(|p| super::picking::resource(client, p))
    } else {
        None
    };
    let mut targets: [Option<([f64; 2], f64)>; 4] = [None; 4];
    let camera_chunk = [
        (client.camera.center[0] as i32).div_euclid(aoe_map::CHUNK_TILES),
        (client.camera.center[1] as i32).div_euclid(aoe_map::CHUNK_TILES),
    ];
    values[3] = if villager {
        if hover.is_some() {
            1.0
        } else if client.pointer.is_some_and(|p| passable(client, p)) {
            2.0
        } else {
            3.0
        }
    } else {
        0.0
    };
    if !client.village.move_cursor
        && client.selected.units.iter().any(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.hp > 0 && u.player == owner)
        })
        && let Some(point) = client.pointer
    {
        let actor = super::picking::actor(client, point);
        if actor.is_some_and(|a| a.1) {
            values[3] = if villager { 5.0 } else { 4.0 };
        } else if actor.is_some_and(|a| a.2)
            || super::picking::enemy_building(client, point).is_some()
        {
            values[3] = 4.0;
        }
    }
    // Starting-kit nodes live in the resource overlay, not in terrain chunks.
    let starting: Vec<_> = client.resources.starting_nodes().collect();
    let natural = client
        .terrain_chunks
        .values()
        .flat_map(|chunk| chunk.resources().map(|(node, _)| node));
    if let Some((kind, remaining)) = client
        .selected
        .resource_id
        .and_then(|id| starting_selection(id, &client.resources))
    {
        // Starting nodes leave the overlay list at zero; the selection stays.
        values[1] = kind;
        values[2] = remaining;
    }
    for node in natural.chain(starting.iter()) {
        if client.selected.resource_id == Some(node.id) {
            values[1] = node.kind as u8 as f64;
            values[2] = f64::from(client.resources.remaining(node.id, node.initial_amount));
        }
        if !client.resources.visible(node.id) {
            continue;
        }
        let world = [f64::from(node.tile.x) + 0.5, f64::from(node.tile.y) + 0.5];
        let chunk = (
            node.tile.x.div_euclid(aoe_map::CHUNK_TILES),
            node.tile.y.div_euclid(aoe_map::CHUNK_TILES),
        );
        if let Some(focus) = focus
            && chunk.0.abs_diff(camera_chunk[0]) <= 1
            && chunk.1.abs_diff(camera_chunk[1]) <= 1
        {
            let distance = (world[0] - focus[0]).powi(2) + (world[1] - focus[1]).powi(2);
            let slot = &mut targets[node.kind as usize];
            if slot.is_none_or(|(_, old)| distance < old) {
                *slot = Some((world, distance));
            }
        }
        let highlighted = hover.is_some_and(|(id, _)| id == node.id);
        if node.kind as u8 == 1 && !highlighted {
            continue;
        }
        let p = map::screen_position(client, world);
        let visible = p.x >= 0.0
            && p.y >= 0.0
            && p.x < client.camera.viewport[0]
            && p.y < client.camera.viewport[1];
        if highlighted {
            values[4] = p.x * scale;
            values[5] = p.y * scale;
            values[8] = (aoe_core::ISO_TILE_WIDTH * client.camera.zoom * scale).max(10.0);
        }
        // Forests are already in the terrain raster. Show other visible
        // deposits only, bounded by resident chunks and the viewport.
        if node.kind as u8 != 1 && visible && values[7] < 2048.0 {
            values[7] += 1.0;
            marker(&mut values, node.kind as u8 as f64, world);
        }
    }
    for (kind, target) in targets.into_iter().enumerate() {
        if let Some((world, distance)) = target {
            let p = map::screen_position(client, world);
            values.extend_from_slice(&[kind as f64, p.x * scale, p.y * scale, distance]);
        }
    }
    values
}

/// Kind and authoritative remaining amount of a selected starting-kit resource,
/// including one at zero that the overlay list no longer yields.
pub(super) fn starting_selection(id: u64, resources: &ResourceStateCache) -> Option<(f64, f64)> {
    let node = aoe_map::ResourceNode::starting_from_id(id)?;
    Some((
        f64::from(node.kind as u8),
        f64::from(resources.remaining(id, node.initial_amount)),
    ))
}

fn passable(client: &Client, point: ScreenPoint) -> bool {
    if client.map_content_hash.is_some()
        && aoe_rendering::surface_hit_at(&map::scene::prepare(client).triangles, point)
            .is_none_or(|hit| !hit.1)
    {
        return false;
    }
    let Some(p) = map::world_at_screen(client, point) else {
        return false;
    };
    let tile = TileCoord::new(p[0].floor() as i32, p[1].floor() as i32);
    let ground = map::terrain_tile(client, tile.x, tile.y)
        .map_or(client.map_content_hash.is_none(), |t| t.passable);
    let resource = map::feedback::blocker_at(client, tile).is_some();
    let building = client.village.buildings.iter().any(|b| {
        tile.x >= b.footprint.min.x
            && tile.y >= b.footprint.min.y
            && tile.x < b.footprint.max.x
            && tile.y < b.footprint.max.y
    });
    aoe_simulation::destination_passable(ground, resource, building)
}

#[inline(never)]
fn marker(values: &mut Vec<f64>, kind: f64, position: [f64; 2]) {
    values.extend_from_slice(&[kind, position[0], position[1]]);
}
