//! Presentation and input only; the server owns the village economy.
use super::Client;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{GameplayBuildingState, GameplayPlayerState, ResourceKind};
use std::collections::BTreeMap;

pub(super) mod hud;
pub(super) mod orders;
pub(super) mod picking;
pub(super) mod selection;

#[derive(Default)]
pub(crate) struct Village {
    pub players: Vec<GameplayPlayerState>,
    pub buildings: Vec<GameplayBuildingState>,
    pub jobs: BTreeMap<EntityId, ResourceKind>,
    pub tick: u64,
    pub last_hud: f64,
    pub move_cursor: bool,
    pub focused: bool,
}

pub(super) fn owner(client: &Client) -> PlayerId {
    client
        .primary
        .and_then(|id| client.units.get(&id))
        .map_or(PlayerId(0), |u| u.player)
}

pub(super) fn building_center(building: &GameplayBuildingState) -> [f64; 2] {
    [
        f64::from(building.footprint.min.x) + 2.0,
        f64::from(building.footprint.min.y) + 2.0,
    ]
}

pub(super) fn resource_kind(client: &Client, id: EntityId) -> Option<ResourceKind> {
    let unit = client.units.get(&id)?;
    unit.carried
        .map(|load| load.kind)
        .or_else(|| client.village.jobs.get(&id).copied())
        .or_else(|| {
            let p = unit.position.as_tiles();
            let tile = unit.position.tile_floor();
            let cx = tile.x.div_euclid(aoe_map::CHUNK_TILES);
            let cy = tile.y.div_euclid(aoe_map::CHUNK_TILES);
            (-1..=1)
                .flat_map(|dy| {
                    (-1..=1).filter_map(move |dx| client.terrain_chunks.get(&(cx + dx, cy + dy)))
                })
                .flat_map(|chunk| chunk.resources())
                .filter_map(|(node, _)| {
                    let dx = f64::from(node.tile.x) + 0.5 - p[0];
                    let dy = f64::from(node.tile.y) + 0.5 - p[1];
                    let distance = dx * dx + dy * dy;
                    (distance < 5.0 && client.resources.visible(node.id))
                        .then_some((distance, node.kind as u8))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, kind)| match kind {
                    0 => ResourceKind::Food,
                    1 => ResourceKind::Wood,
                    2 => ResourceKind::Gold,
                    _ => ResourceKind::Stone,
                })
        })
}

#[cfg(test)]
mod tests;
