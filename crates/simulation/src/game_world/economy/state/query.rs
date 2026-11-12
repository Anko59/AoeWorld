//! Building observations project exact construction progress at the phase clock.
use super::super::{BuildingKind, GameBuilding};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId};

impl GameWorld {
    pub fn buildings(&self) -> impl Iterator<Item = std::borrow::Cow<'_, GameBuilding>> {
        self.economy.buildings.values().map(|b| {
            self.economy
                .queues
                .project(self.economy.construction.project(b))
        })
    }
    /// One player's buildings, kind by kind, from the per-player index.
    pub(crate) fn player_buildings(
        &self,
        player: PlayerId,
    ) -> impl Iterator<Item = std::borrow::Cow<'_, GameBuilding>> {
        self.economy
            .dropoffs
            .range((player, BuildingKind::TownCenter)..)
            .take_while(move |((owner, _), _)| *owner == player)
            .flat_map(|(_, ids)| ids.iter())
            .filter_map(|id| self.building(*id))
    }
    /// One player's buildings of one kind, ascending by id.
    pub(crate) fn player_buildings_of(
        &self,
        player: PlayerId,
        kind: BuildingKind,
    ) -> impl Iterator<Item = std::borrow::Cow<'_, GameBuilding>> {
        self.economy
            .dropoffs
            .get(&(player, kind))
            .into_iter()
            .flatten()
            .filter_map(|id| self.building(*id))
    }
    pub fn building(&self, id: EntityId) -> Option<std::borrow::Cow<'_, GameBuilding>> {
        self.economy.buildings.get(&id).map(|b| {
            self.economy
                .queues
                .project(self.economy.construction.project(b))
        })
    }
}
