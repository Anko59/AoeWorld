//! Placement feedback and construction orders use only resident map knowledge.
use super::{Client, orders::send, owner};
use crate::{building_ui, playground::game::map};
use aoe_core::{EntityId, ScreenPoint, TileCoord, TileRect};
pub(crate) mod strategic;

use aoe_protocol::{BuildingKind, ConstructionState, GameplayClientMessage as Message, UnitKind};

#[inline(never)]
pub(super) fn builders(client: &Client) -> Vec<EntityId> {
    selected_of(client, UnitKind::Villager)
}

/// Selected own Fishing Ships, the builders of Fish Traps.
#[inline(never)]
pub(super) fn ship_builders(client: &Client) -> Vec<EntityId> {
    selected_of(client, UnitKind::FishingShip)
}

fn selected_of(client: &Client, kind: UnitKind) -> Vec<EntityId> {
    client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.hp > 0 && u.kind == kind)
        })
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
pub(super) fn allows_land_placement(client: &Client) -> bool {
    crate::playground::game::strategic::active(client)
        || client.selected.units.iter().any(|id| {
            client
                .units
                .get(id)
                .is_some_and(|u| u.player == owner(client) && u.kind == UnitKind::Villager)
        })
}

#[inline(never)]
pub(super) fn enabled(client: &Client, kind: BuildingKind, count: u32) -> bool {
    let Some(player) = client
        .village
        .players
        .iter()
        .find(|p| p.player == owner(client))
    else {
        return false;
    };
    let land = allows_land_placement(client);
    let fishing = !ship_builders(client).is_empty();
    building_ui::availability::BuildMenu::new(Some(player), completed(client), land, fishing)
        .enabled(kind, count)
}

#[inline(never)]
pub(super) fn arm(client: &mut Client, kind: BuildingKind) {
    if !enabled(client, kind, 1) {
        return;
    }
    client.village.garrison_cursor = false;
    client.village.far_placement = crate::playground::game::strategic::active(client);
    client.village.placement = Some(kind);
    client.village.repeat_placement = false;
    client.village.wall_start = None;
    client.village.clear_move_cursor();
    client.village.unload_cursor = false;
    let _ = client.canvas.set_attribute("data-order-mode", "build");
}

/// Ends a placement and every other armed cursor with the one on the screen.
#[inline(never)]
pub(in super::super) fn cancel(client: &mut Client) {
    client.village.clear_move_cursor();
    client.village.garrison_cursor = false;
    client.village.unload_cursor = false;
    client.village.far_placement = false;
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
    let footprint = TileRect::from_xywh(origin.x, origin.y, kind.side(), kind.depth());
    if super::relics::ground(client)
        .iter()
        .any(|relic| footprint.contains(relic.position.tile_floor()))
    {
        return false;
    }
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
                    && !matches!(
                        kind,
                        BuildingKind::Dock | BuildingKind::Bridge | BuildingKind::FishTrap
                    ),
                |t| {
                    let wet = t.water != aoe_map::WaterKind::None;
                    water |= wet;
                    land |= t.passable && t.water == aoe_map::WaterKind::None;
                    // A Fish Trap stands on open water only.
                    if kind == BuildingKind::FishTrap {
                        wet
                    } else if t.material == aoe_map::GroundMaterial::Ford && !crosses_shallows(kind)
                    {
                        // DE: nothing but walls, roads and gates stands on shallows.
                        false
                    } else {
                        t.passable
                            || (matches!(kind, BuildingKind::Dock | BuildingKind::Bridge) && wet)
                    }
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

/// Wall and road segments and gates may cross a ford; the server's ground rule
/// matches (`validate_dry_footprint`).
fn crosses_shallows(kind: BuildingKind) -> bool {
    kind.is_line() || kind.is_gate()
}

#[inline(never)]
pub(super) fn tiles(client: &Client, at: TileCoord) -> Option<Vec<TileCoord>> {
    if client.village.placement == Some(BuildingKind::Bridge) {
        let from = client.village.wall_start.unwrap_or(at);
        aoe_core::snap_bridge_start(
            from,
            at,
            |tile| {
                map::terrain_tile(client, tile.x, tile.y)
                    .map(|t| t.water != aoe_map::WaterKind::None)
            },
            |span| {
                let tiles = span.tiles();
                span_valid(client, BuildingKind::Bridge, &tiles)
            },
        )
        .map(|span| span.tiles())
        .or_else(|| aoe_core::raster_bridge_line(from, at))
    } else if client.village.placement.is_some_and(BuildingKind::is_line) {
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
        && (!(kind.is_line() || kind == BuildingKind::Bridge)
            || client.village.wall_start.is_none())
    {
        return;
    }
    let Some(at) = origin(client, point) else {
        return;
    };
    if kind.is_line() || kind == BuildingKind::Bridge {
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
    let far = crate::playground::game::strategic::active(client);
    let sequence = client.next_sequence;
    let builder_ids = if far {
        Vec::new()
    } else if kind == BuildingKind::FishTrap {
        ship_builders(client)
    } else {
        builders(client)
    };
    if !builder_group_fits(client, builder_ids.len()) {
        return;
    }
    let message = if kind == BuildingKind::Bridge {
        Message::BuildBridgeOrder {
            sequence,
            builder_ids,
            from: tiles[0],
            to: tiles[tiles.len() - 1],
            queued: shift,
        }
    } else if kind.is_line() {
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
    if send(client, message) && far {
        client.village.pending_far_builds.insert(sequence);
        strategic::submitting();
    }
    if shift {
        client.village.repeat_placement = true;
        client.village.wall_start = None;
    } else {
        cancel(client);
    }
}

fn builder_group_fits(client: &mut Client, count: usize) -> bool {
    if count <= aoe_protocol::MAX_BUILD_GROUP {
        return true;
    }
    client.status = "Select fewer workers for this building order.".into();
    false
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
    let mut ids = builders(client);
    if ids.is_empty() {
        // Fishing Ships build and work Fish Traps the way villagers do farms.
        ids = ship_builders(client);
    }
    if ids.is_empty() || client.village.move_cursor {
        return false;
    }
    let Some(b) = target(client, point) else {
        return false;
    };
    let worker = if b.kind == BuildingKind::FishTrap {
        UnitKind::FishingShip
    } else {
        UnitKind::Villager
    };
    if client.units.get(&ids[0]).is_none_or(|u| u.kind != worker) {
        return false;
    }
    let (id, unfinished, farm) = (
        b.id,
        b.construction == ConstructionState::UnderConstruction || b.hp < b.max_hp,
        matches!(b.kind, BuildingKind::Farm | BuildingKind::FishTrap),
    );
    if unfinished {
        if !builder_group_fits(client, ids.len()) {
            return true;
        }
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
