//! Presentation and input only; the server owns the village economy.
use super::Client;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{GameplayBuildingState, GameplayPlayerState, ResourceKind};

pub(super) mod hud;
pub(super) mod orders;
pub(super) mod picking;
pub(super) mod presentation;
pub(super) mod selection;

#[derive(Default)]
pub(crate) struct Village {
    pub players: Vec<GameplayPlayerState>,
    pub buildings: Vec<GameplayBuildingState>,
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
    client.units.get(&id)?.gathering
}

#[cfg(test)]
mod tests;
