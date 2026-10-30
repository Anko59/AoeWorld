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
            client.units.get(id).is_some_and(|u| {
                u.player == owner(client) && u.hp > 0 && u.kind == UnitKind::Villager
            })
        })
        .take(aoe_protocol::MAX_BUILD_GROUP)
        .collect()
}

#[inline(never)]
pub(super) fn completed(client: &Client) -> u32 {
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
    cost.stone = cost.stone.saturating_mul(count);
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
    client.village.garrison_cursor = false;
    client.village.placement = Some(kind);
    client.village.repeat_placement = false;
    client.village.wall_start = None;
    client.village.move_cursor = false;
    client.village.unload_cursor = false;
    client.village.attack_move = false;
    let _ = client.canvas.set_attribute("data-order-mode", "build");
}

#[inline(never)]
pub(in super::super) fn cancel(client: &mut Client) {
    client.village.garrison_cursor = false;
    client.village.placement = None;
    client.village.repeat_placement = false;
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
            if client.fog.state([tile.x, tile.y]) == 0 {
                return false;
            }
            let ground = map::terrain_tile(client, tile.x, tile.y).map_or(
                client.map_content_hash.is_none()
                    && !matches!(kind, BuildingKind::Dock | BuildingKind::Bridge),
                |t| {
                    let wet = t.water != aoe_map::WaterKind::None;
                    water |= wet;
                    land |= t.passable && t.water == aoe_map::WaterKind::None;
                    t.passable || (matches!(kind, BuildingKind::Dock | BuildingKind::Bridge) && wet)
                },
            );
            ground
                // Fish remain gather targets, but do not collide with Docks or decks.
                && map::feedback::blocker_at(client, tile).is_none_or(|object| {
                    matches!(object, aoe_map::ObjectKind::ShoreFish | aoe_map::ObjectKind::DeepFish)
                })
                // Units never invalidate a foundation (they step aside), except
                // under a Dock, which the server still refuses over units.
                && (kind != BuildingKind::Dock
                    || !client
                        .units
                        .values()
                        .any(|u| u.hp > 0 && u.position.tile_floor() == tile))
                && !client.village.buildings.iter().any(|b| b.contains_tile(tile))
        },
    );
    valid && (kind != BuildingKind::Dock || land && water)
}

#[inline(never)]
pub(super) fn tiles(client: &Client, at: TileCoord) -> Option<Vec<TileCoord>> {
    if client.village.placement == Some(BuildingKind::Bridge) {
        let from = client.village.wall_start.unwrap_or(at);
        aoe_core::raster_bridge_line(from, at)
    } else if client.village.placement.is_some_and(BuildingKind::is_wall) {
        building_ui::wall_tiles(client.village.wall_start.unwrap_or(at), at)
    } else {
        Some(vec![at])
    }
}

#[inline(never)]
pub(super) fn tile_valid(
    client: &Client,
    kind: BuildingKind,
    tiles: &[TileCoord],
    i: usize,
) -> bool {
    let tile = tiles[i];
    valid(client, kind, tile)
        && (kind != BuildingKind::Bridge
            || map::terrain_tile(client, tile.x, tile.y).is_some_and(|t| {
                if i == 0 || i + 1 == tiles.len() {
                    t.passable && t.water == aoe_map::WaterKind::None
                } else {
                    t.water != aoe_map::WaterKind::None
                }
            }))
}

pub(super) fn span_valid(client: &Client, kind: BuildingKind, tiles: &[TileCoord]) -> bool {
    let bridge = kind == BuildingKind::Bridge;
    let count = if bridge {
        tiles.len().saturating_sub(2)
    } else {
        tiles.len()
    };
    count > 0
        && (!bridge
            || tiles
                .first()
                .zip(tiles.last())
                .is_some_and(|(&from, &to)| aoe_core::BridgeSpan::new(from, to).is_some()))
        && enabled(client, kind, count as u32)
        && (0..tiles.len()).all(|i| tile_valid(client, kind, tiles, i))
}

#[inline(never)]
pub(in super::super) fn place(client: &mut Client, point: ScreenPoint, release: bool, shift: bool) {
    let Some(kind) = client.village.placement else {
        return;
    };
    client.village.repeat_placement |= shift;
    if release
        && (!(kind.is_wall() || kind == BuildingKind::Bridge)
            || client.village.wall_start.is_none())
    {
        return;
    }
    let Some(at) = origin(client, point) else {
        return;
    };
    if kind.is_wall() || kind == BuildingKind::Bridge {
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
    if !span_valid(client, kind, &tiles) {
        return;
    }
    let sequence = client.next_sequence;
    let builder_ids = builders(client);
    let message = if kind == BuildingKind::Bridge {
        Message::BuildBridgeOrder {
            sequence,
            builder_ids,
            from: client.village.wall_start.unwrap_or(at),
            to: at,
            queued: shift,
        }
    } else if kind.is_wall() {
        Message::BuildWallOrder {
            sequence,
            builder_ids,
            kind,
            from: client.village.wall_start.unwrap_or(at),
            to: at,
            queued: shift,
        }
    } else {
        Message::BuildOrder {
            sequence,
            builder_ids,
            kind,
            origin: at,
            queued: shift,
        }
    };
    send(client, message);
    if shift {
        client.village.repeat_placement = true;
        client.village.wall_start = None;
    } else {
        cancel(client);
    }
}

/// Foundations take precedence over gather/move; completed farms receive a farmer.
pub(super) fn target(
    client: &Client,
    point: ScreenPoint,
) -> Option<&aoe_protocol::GameplayBuildingState> {
    let at = origin(client, point)?;
    client
        .village
        .buildings
        .iter()
        .find(|b| b.player == owner(client) && b.footprint.contains(at))
}

#[inline(never)]
pub(super) fn interact(client: &mut Client, point: ScreenPoint) -> bool {
    let ids = builders(client);
    if ids.is_empty() || client.village.move_cursor {
        return false;
    }
    let Some(b) = target(client, point) else {
        return false;
    };
    let (id, unfinished, farm) = (
        b.id,
        b.construction == ConstructionState::UnderConstruction || b.hp < b.max_hp,
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
