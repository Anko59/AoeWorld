//! Placement feedback and construction orders use only resident map knowledge.
use super::{Client, orders::send, owner};
use crate::{building_ui, playground::game::map};
use aoe_core::{EntityId, ScreenPoint, TileCoord};
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayClientMessage as Message, GameplayRole, UnitKind,
};

#[inline(never)]
pub(super) fn builders(client: &Client) -> Vec<EntityId> {
    client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.kind == UnitKind::Villager)
        })
        .take(aoe_protocol::MAX_BUILD_GROUP)
        .collect()
}

#[inline(never)]
pub(super) fn completed(client: &Client) -> u16 {
    client
        .village
        .buildings
        .iter()
        .filter(|b| b.player == owner(client) && b.construction == ConstructionState::Complete)
        .fold(0, |mask, b| mask | (1 << b.kind as u8))
}

#[inline(never)]
pub(super) fn enabled(client: &Client, kind: BuildingKind, count: u32) -> bool {
    let Some(p) = client
        .village
        .players
        .iter()
        .find(|p| p.player == owner(client))
    else {
        return false;
    };
    let mut cost = building_ui::stock(building_ui::rules(kind).cost);
    cost.wood = cost.wood.saturating_mul(count);
    client.role == Some(GameplayRole::Controller)
        && client.selected.units.iter().any(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.kind == UnitKind::Villager)
        })
        && building_ui::build_enabled(kind, p.stockpile, completed(client), p.age)
        && building_ui::affordable(p.stockpile, cost)
}

#[inline(never)]
pub(super) fn arm(client: &mut Client, kind: BuildingKind) {
    if !enabled(client, kind, 1) {
        return;
    }
    client.village.placement = Some(kind);
    client.village.wall_start = None;
    client.village.move_cursor = false;
    let _ = client.canvas.set_attribute("data-order-mode", "build");
}

#[inline(never)]
pub(in super::super) fn cancel(client: &mut Client) {
    client.village.placement = None;
    client.village.wall_start = None;
    let _ = client.canvas.set_attribute("data-order-mode", "");
}

#[inline(never)]
pub(super) fn origin(client: &Client, point: ScreenPoint) -> Option<TileCoord> {
    let p = map::world_at_screen(client, point)?;
    Some(TileCoord::new(p[0].floor() as i32, p[1].floor() as i32))
}

#[inline(never)]
pub(super) fn valid(client: &Client, kind: BuildingKind, origin: TileCoord) -> bool {
    let (mut land, mut water) = (false, false);
    let valid = building_ui::footprint_valid(
        kind,
        origin,
        [client.config.width_tiles, client.config.height_tiles],
        |tile| {
            let ground = map::terrain_tile(client, tile.x, tile.y).map_or(
                client.map_content_hash.is_none() && kind != BuildingKind::Dock,
                |t| {
                    let wet = matches!(
                        t.water,
                        aoe_map::WaterKind::Lake
                            | aoe_map::WaterKind::Ocean
                            | aoe_map::WaterKind::River
                    ) && t.material != aoe_map::GroundMaterial::Ford;
                    water |= wet;
                    land |= t.passable && t.water == aoe_map::WaterKind::None;
                    t.passable || (kind == BuildingKind::Dock && wet)
                },
            );
            ground
                && map::feedback::blocker_at(client, tile).is_none()
                && !client
                    .units
                    .values()
                    .any(|u| u.position.tile_floor() == tile)
                && !client.village.buildings.iter().any(|b| {
                    tile.x >= b.footprint.min.x
                        && tile.y >= b.footprint.min.y
                        && tile.x < b.footprint.max.x
                        && tile.y < b.footprint.max.y
                })
        },
    );
    valid && (kind != BuildingKind::Dock || land && water)
}

#[inline(never)]
pub(super) fn tiles(client: &Client, at: TileCoord) -> Option<Vec<TileCoord>> {
    if client.village.placement == Some(BuildingKind::PalisadeWall) {
        building_ui::wall_tiles(client.village.wall_start.unwrap_or(at), at)
    } else {
        Some(vec![at])
    }
}

#[inline(never)]
pub(in super::super) fn place(client: &mut Client, point: ScreenPoint, release: bool) {
    let Some(kind) = client.village.placement else {
        return;
    };
    let Some(at) = origin(client, point) else {
        return;
    };
    if kind == BuildingKind::PalisadeWall {
        if !release && client.village.wall_start.is_none() {
            client.village.wall_start = Some(at);
            return;
        }
        // A stationary release keeps the first endpoint armed: a second click
        // also finishes a line, for pointer devices that cannot hold a drag.
        if release && client.village.wall_start == Some(at) {
            return;
        }
    }
    let Some(tiles) = tiles(client, at) else {
        return;
    };
    if !enabled(client, kind, tiles.len() as u32) || !tiles.iter().all(|t| valid(client, kind, *t))
    {
        return;
    }
    let sequence = client.next_sequence;
    let builder_ids = builders(client);
    let message = if kind == BuildingKind::PalisadeWall {
        Message::BuildWallOrder {
            sequence,
            builder_ids,
            from: client.village.wall_start.unwrap_or(at),
            to: at,
        }
    } else {
        Message::BuildOrder {
            sequence,
            builder_ids,
            kind,
            origin: at,
        }
    };
    send(client, message);
    cancel(client);
}

/// Foundations take precedence over gather/move; completed farms receive a farmer.
#[inline(never)]
pub(super) fn interact(client: &mut Client, point: ScreenPoint) -> bool {
    let ids = builders(client);
    if ids.is_empty() || client.village.move_cursor {
        return false;
    }
    let Some(at) = origin(client, point) else {
        return false;
    };
    let Some(b) = client.village.buildings.iter().find(|b| {
        b.player == owner(client)
            && at.x >= b.footprint.min.x
            && at.y >= b.footprint.min.y
            && at.x < b.footprint.max.x
            && at.y < b.footprint.max.y
    }) else {
        return false;
    };
    let (id, unfinished, farm) = (
        b.id,
        b.construction == ConstructionState::UnderConstruction,
        b.kind == BuildingKind::Farm,
    );
    if unfinished {
        let sequence = client.next_sequence;
        send(
            client,
            Message::ConstructOrder {
                sequence,
                builder_ids: ids,
                building_id: id,
            },
        );
    } else if farm {
        let sequence = client.next_sequence;
        send(
            client,
            Message::FarmOrder {
                sequence,
                entity_id: ids[0],
                farm_id: id,
            },
        );
    }
    unfinished || farm
}
