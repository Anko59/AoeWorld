//! Presentation and input only; the server owns the village economy.
use super::Client;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{GameplayBuildingState, GameplayPlayerState, ResourceKind};

pub(super) mod hud;
pub(super) mod orders;
pub(super) mod picking;
pub(super) mod selection;

#[derive(Default)]
pub(crate) struct Village {
    pub players: Vec<GameplayPlayerState>,
    pub buildings: Vec<GameplayBuildingState>,
    pub jobs: Vec<(EntityId, ResourceKind)>,
    pub tick: u64,
    pub last_hud: f64,
    pub move_cursor: bool,
    pub focused: bool,
}

impl Village {
    pub fn set_job(&mut self, id: EntityId, resource: Option<ResourceKind>) {
        self.jobs.retain(|(unit, _)| *unit != id);
        if let Some(kind) = resource {
            self.jobs.push((id, kind));
        }
    }
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
        .or_else(|| {
            client
                .village
                .jobs
                .iter()
                .find(|(unit, _)| *unit == id)
                .map(|(_, kind)| *kind)
        })
        .or_else(|| {
            let p = unit.position.as_tiles();
            let tile = unit.position.tile_floor();
            let cx = tile.x.div_euclid(aoe_map::CHUNK_TILES);
            let cy = tile.y.div_euclid(aoe_map::CHUNK_TILES);
            let mut distance = 5.0;
            let mut nearest = None;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(chunk) = client.terrain_chunks.get(&(cx + dx, cy + dy)) else {
                        continue;
                    };
                    for (node, _) in chunk.resources() {
                        let dx = f64::from(node.tile.x) + 0.5 - p[0];
                        let dy = f64::from(node.tile.y) + 0.5 - p[1];
                        let candidate = dx * dx + dy * dy;
                        if candidate < distance && client.resources.visible(node.id) {
                            distance = candidate;
                            nearest = Some(node.kind as u8);
                        }
                    }
                }
            }
            nearest.map(|kind| match kind {
                0 => ResourceKind::Food,
                1 => ResourceKind::Wood,
                2 => ResourceKind::Gold,
                _ => ResourceKind::Stone,
            })
        })
}

#[cfg(test)]
mod tests;
