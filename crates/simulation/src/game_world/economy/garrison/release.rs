use super::*;

impl GameWorld {
    /// `None` ejects all; `Some(id)` ejects one of this building's occupants.
    /// Blocked exits retain their occupants, so commands and destruction never lose units.
    pub fn issue_ungarrison(
        &mut self,
        player: PlayerId,
        building: EntityId,
        unit: Option<EntityId>,
    ) -> Result<(), EconomyError> {
        let shelter = self.building(building).ok_or(EconomyError::UnknownEntity)?;
        if shelter.player != player {
            return Err(EconomyError::NotOwner);
        }
        if shelter.kind.garrison_capacity() == 0 {
            return Err(EconomyError::WrongUnitKind);
        }
        if unit.is_some_and(|id| !shelter.garrison.iter().any(|g| g.unit.id == id)) {
            return Err(EconomyError::InvalidTarget);
        }
        let footprint = shelter.footprint;
        let Some(shelter) = self.economy.buildings.get_mut(&building) else {
            return Err(EconomyError::UnknownEntity);
        };
        let occupants = std::mem::take(&mut shelter.garrison);
        let mut retained = Vec::new();
        for occupant in occupants {
            if unit.is_some_and(|id| id != occupant.unit.id)
                || !self.eject_occupant(footprint, occupant, false)
            {
                retained.push(occupant);
            }
        }
        if let Some(shelter) = self.economy.buildings.get_mut(&building) {
            shelter.garrison = retained;
        } else {
            self.release_destroyed_garrison(building, footprint, retained, &mut Vec::new());
        }
        self.refresh_garrison_fire(building);
        self.remember_visible_buildings();
        Ok(())
    }

    fn eject_occupant(
        &mut self,
        footprint: TileRect,
        occupant: GarrisonedUnit,
        forced: bool,
    ) -> bool {
        // Local, canonical rings; queries visit occupied spatial buckets only.
        // Expanding four rings leaves room even when the first exit is busy.
        let tile = (0..4)
            .flat_map(|margin| {
                perimeter(TileRect::from_xywh(
                    footprint.min.x - margin,
                    footprint.min.y - margin,
                    footprint.width() + 2 * margin,
                    footprint.height() + 2 * margin,
                ))
            })
            .find(|tile| {
                self.unit_tile_passable(occupant.unit.kind, *tile, &|| false)
                    .unwrap_or(false)
                    && !self.economy.blocked_tiles.contains(tile)
                    && self
                        .query(TileRect::from_xywh(tile.x, tile.y, 1, 1))
                        .0
                        .is_empty()
            })
            .or_else(|| {
                // Destruction may stack survivors on its newly opened footprint
                // when every other exit is busy, rather than leave units hidden.
                (footprint.min.y..footprint.max.y)
                    .flat_map(|y| {
                        (footprint.min.x..footprint.max.x)
                            .map(move |x| aoe_core::TileCoord::new(x, y))
                    })
                    .find(|tile| {
                        (self.tile_free(*tile) || forced && self.tile_passable(*tile))
                            && self
                                .unit_tile_passable(occupant.unit.kind, *tile, &|| false)
                                .unwrap_or(false)
                    })
            });
        let Some(at) = tile.and_then(|tile| position(tile).ok()) else {
            return false;
        };
        let mut state = occupant.unit;
        state.position = at;
        state.previous_position = at;
        self.insert_map_unit(state, occupant.next_hit_tick);
        true
    }

    pub(crate) fn release_destroyed_garrison(
        &mut self,
        id: EntityId,
        footprint: TileRect,
        occupants: Vec<GarrisonedUnit>,
        changed: &mut Vec<EntityId>,
    ) {
        self.economy.occupied_garrisons.remove(&id);
        let mut retained = Vec::new();
        for occupant in occupants {
            if self.eject_occupant(footprint, occupant, true) {
                changed.push(occupant.unit.id);
            } else {
                retained.push(occupant);
            }
        }
        if !retained.is_empty() {
            self.economy.ejections.insert(id, (footprint, retained));
        }
    }

    pub(super) fn refresh_garrison_fire(&mut self, id: EntityId) {
        if self.building(id).is_some_and(|b| b.garrison.is_empty()) {
            self.economy.occupied_garrisons.remove(&id);
            // Keep the building's reload tick across entry/ejection cycles.
            // Empty Town Centers skip target queries in advance_towers.
        }
    }
}
