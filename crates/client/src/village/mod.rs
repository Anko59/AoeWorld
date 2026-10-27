//! Presentation and input only; the server owns the village economy.
use super::Client;
use aoe_core::PlayerId;
use aoe_protocol::{GameplayBuildingState, GameplayPlayerState, UnitKind};

pub(super) mod automation;
pub(super) mod building_panel;
pub(super) mod construction;
pub(super) mod garrison;
pub(super) mod hud;
pub(super) mod navy;
pub(super) mod orders;
pub(super) mod picking;
pub(super) mod presentation;
#[path = "production/mod.rs"]
pub(super) mod production;
pub(super) mod rally;
pub(super) mod selection;
pub(super) mod vassal;
pub(super) mod waypoints;

#[derive(Default)]
pub(crate) struct Village {
    pub idle_villagers: selection::IdleFinder,
    pub idle_military: selection::IdleFinder,
    pub viewer: std::cell::Cell<Option<PlayerId>>,
    pub move_queues: Vec<aoe_protocol::MoveQueue>,
    pub players: Vec<GameplayPlayerState>,
    pub buildings: Vec<GameplayBuildingState>,
    pub tick: u64,
    pub pending_training: production::PendingTraining,
    pub last_hud: f64,
    pub move_cursor: bool,
    pub garrison_cursor: bool,
    pub unload_cursor: bool,
    pub formation: aoe_core::Formation,
    pub attack_move: bool,
    pub hovered_resource: std::cell::Cell<Option<u64>>,
    pub focused: bool,
    pub placement: Option<aoe_protocol::BuildingKind>,
    pub repeat_placement: bool,
    pub wall_start: Option<aoe_core::TileCoord>,
    pub rubble: Vec<(GameplayBuildingState, u64)>,
    pub animals: Vec<aoe_protocol::AnimalState>,
    pub wildlife_tick: u64,
    pub projectiles: Vec<aoe_protocol::ProjectileState>,
    pub projectile_tick: u64,
    pub gate_art: std::cell::RefCell<crate::building_ui::GateArt>,
}

impl Village {
    pub fn remember_idle(&mut self, owner: PlayerId, unit: &aoe_protocol::GameplayUnitState) {
        self.idle_villagers.update(
            unit,
            unit.player == owner && unit.kind == UnitKind::Villager,
        );
        self.idle_military.update(
            unit,
            unit.player == owner
                && !matches!(
                    unit.kind,
                    UnitKind::Villager | UnitKind::FishingShip | UnitKind::TransportShip
                ),
        );
    }
}

#[inline(never)]
pub(super) fn owner(client: &Client) -> PlayerId {
    let player = client
        .primary
        .and_then(|id| client.units.get(&id))
        .map(|u| u.player)
        // The primary scout may be inside a Castle and absent from map units.
        // Gameplay economies are filtered to the viewer's own player.
        .or_else(|| client.village.players.first().map(|p| p.player));
    if let Some(player) = player {
        client.village.viewer.set(Some(player));
    }
    player
        .or_else(|| client.village.viewer.get())
        .unwrap_or(PlayerId(0))
}

pub(super) fn building_center(building: &GameplayBuildingState) -> [f64; 2] {
    [
        f64::from(building.footprint.min.x) + f64::from(building.footprint.width()) / 2.0,
        f64::from(building.footprint.min.y) + f64::from(building.footprint.height()) / 2.0,
    ]
}

#[cfg(test)]
mod tests;
