//! Rally input and the selected building's authoritative flag.
use super::{Client, owner};
use crate::playground::game::map;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, ScreenPoint, WorldPosition};
use aoe_protocol::{GameplayClientMessage as Message, RallyTarget};

/// Every selected training building of the owner takes the same rally, not
/// only the primary one.
pub(super) fn rally_buildings(
    buildings: &[aoe_protocol::GameplayBuildingState],
    selected: &super::selection::Selection,
    player: aoe_core::PlayerId,
) -> Vec<aoe_core::EntityId> {
    buildings
        .iter()
        .filter(|b| selected.has_building(b.id) && b.player == player)
        .filter(|b| {
            aoe_protocol::UnitKind::ALL
                .iter()
                .any(|kind| crate::building_ui::rules(b.kind).trains(*kind as u8))
        })
        .map(|b| b.id)
        .collect()
}

pub(super) fn at_pointer(client: &mut Client, point: ScreenPoint) -> bool {
    let ids = rally_buildings(&client.village.buildings, &client.selected, owner(client));
    if ids.is_empty() {
        return false;
    }
    let Some(world) = map::world_at_screen(client, point) else {
        return true;
    };
    let Ok(position) = WorldPosition::from_i64(
        (world[0] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
        (world[1] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
    ) else {
        return true;
    };
    let position = client.config.snap_ground_position(position);
    let animal = super::picking::actor(client, point)
        .filter(|a| a.1)
        .map(|a| a.0);
    let friendly = client
        .village
        .buildings
        .iter()
        .find(|b| b.player == owner(client) && b.footprint.contains(position.tile_floor()))
        .map(|b| b.id);
    let resource = super::picking::resource(client, point).map(|r| r.0);
    let target = if let Some(id) = animal {
        RallyTarget::Animal(id)
    } else if let Some(id) = friendly {
        RallyTarget::Building(id)
    } else if let Some(id) = resource {
        RallyTarget::Resource(id)
    } else {
        RallyTarget::Ground(position)
    };
    for building_id in ids {
        let sequence = client.next_sequence;
        super::orders::send(
            client,
            Message::SetRallyOrder {
                sequence,
                building_id,
                target,
            },
        );
    }
    true
}

pub(in super::super) fn marker(client: &Client) -> Option<aoe_rendering::SceneResource> {
    let rally = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id && b.player == owner(client))?
        .rally_point?;
    let position = rally.position.as_tiles();
    Some(aoe_rendering::SceneResource {
        id: u64::MAX,
        position,
        kind: aoe_rendering::SceneResource::RALLY_FLAG,
        visual_variant: ((client.last_frame / 300.0) as u64 % 10) as u8,
        visual_family: 0,
        highlight: 0,
        elevation_meters: map::elevation_at_world(client, position),
    })
}

pub(super) fn present(client: &Client) {
    let point = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id && b.player == owner(client))
        .and_then(|b| b.rally_point);
    let value = point
        .map(|r| format!("{},{}", r.position.x, r.position.y))
        .unwrap_or_default();
    let _ = client.canvas.set_attribute("data-rally-point", &value);
}
