//! Building memory refreshed by events. A watcher's remembered snapshot can
//! only differ from the live building after the building changed or the
//! watcher began to see it, so only those buildings are compared: every
//! mutable building access goes through [`GameWorld::building_mut`] (or marks
//! the id itself), and a watch gain marks its building. No tick clones or
//! compares every watched building any more.
use crate::{GameBuilding, GameWorld};
use aoe_core::EntityId;

impl GameWorld {
    /// The one mutable path to a building: the change reaches the memory of
    /// every player watching it at the next refresh.
    pub(crate) fn building_mut(&mut self, id: EntityId) -> Option<&mut GameBuilding> {
        self.wake_construction(id);
        let building = self.economy.buildings.get_mut(&id)?;
        self.vision.dirty_buildings.insert(id);
        Some(building)
    }

    /// Marks a building whose fields were changed without [`Self::building_mut`].
    pub(crate) fn mark_building_changed(&mut self, id: EntityId) {
        if self.economy.buildings.contains_key(&id) {
            self.vision.dirty_buildings.insert(id);
        }
    }

    /// Marks every building: restores and fog rebuilds re-derive all memory.
    pub(crate) fn mark_all_buildings_changed(&mut self) {
        self.vision
            .dirty_buildings
            .extend(self.economy.buildings.keys().copied());
    }

    /// Copies each changed or newly watched building into the memory of the
    /// players watching it. Returns the buildings visited.
    pub(crate) fn remember_visible_buildings(&mut self) -> usize {
        // Progress changes the externally observed snapshot on every tick even
        // while the crew sleeps. Visit sites for publication, never workers.
        self.vision
            .dirty_buildings
            .extend(self.economy.construction.progressing());
        let dirty = std::mem::take(&mut self.vision.dirty_buildings);
        if self.vision.disabled {
            return 0;
        }
        let visited = dirty.len();
        for id in dirty {
            let (Some(building), Some(watched)) = (
                self.economy.buildings.get(&id),
                self.vision.building_watchers.get(&id),
            ) else {
                continue;
            };
            let building = self.economy.construction.project(building);
            for player in &watched.watchers {
                if let Some(vision) = self.vision.players.get_mut(player)
                    // Only a differing snapshot is a change.
                    && vision.remembered.get(&id) != Some(building.as_ref())
                {
                    vision.changed.insert(id);
                    vision.remembered.insert(id, building.clone().into_owned());
                }
            }
        }
        #[cfg(test)]
        self.assert_building_memory_current();
        visited
    }

    /// Test oracle: the full pass this replaced would find nothing to copy.
    #[cfg(test)]
    fn assert_building_memory_current(&self) {
        for (id, watched) in &self.vision.building_watchers {
            let Some(building) = self.economy.buildings.get(id) else {
                continue;
            };
            let building = self.economy.construction.project(building);
            for player in &watched.watchers {
                if let Some(vision) = self.vision.players.get(player) {
                    assert_eq!(
                        vision.remembered.get(id),
                        Some(building.as_ref()),
                        "building {id:?} changed without building_mut (player {player:?})"
                    );
                }
            }
        }
    }
}
