//! Shore rendezvous and boarding; carried units retain their identity.
use crate::{EconomyError, GameWorld, UnitKind, UnitTask, unit_combat};
use aoe_core::{ChunkCoord, EntityId, PlayerId, TileCoord, WorldPosition};
use std::collections::BTreeSet;

mod combat;
mod journeys;
pub(crate) use journeys::Journey;
mod stats;
pub(super) use stats::NAVAL;
pub(super) use stats::attack_range;

const CAPACITY: usize = 5;

fn adjacent(a: TileCoord, b: TileCoord) -> bool {
    a.x.abs_diff(b.x) <= 1 && a.y.abs_diff(b.y) <= 1
}

impl GameWorld {
    pub fn transport_cargo(&self, transport: EntityId) -> &[EntityId] {
        self.economy
            .cargo
            .get(&transport)
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn embarked(&self, id: EntityId) -> bool {
        self.economy.embarked.contains_key(&id)
    }

    fn validate_transport(
        &self,
        player: PlayerId,
        id: EntityId,
    ) -> Result<TileCoord, EconomyError> {
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit.kind != UnitKind::TransportShip {
            return Err(EconomyError::WrongUnitKind);
        }
        if unit.hp == 0 {
            return Err(EconomyError::InvalidTarget);
        }
        Ok(unit.position.tile_floor())
    }

    /// Validate the entire group before cancelling any task or adding cargo.
    pub fn issue_load(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        transport: EntityId,
    ) -> Result<(), EconomyError> {
        let bank = self.validate_transport(player, transport)?;
        if ids.is_empty() || ids.len() > CAPACITY {
            return Err(EconomyError::OrderTooLarge);
        }
        if self.transport_cargo(transport).len() + ids.len() > CAPACITY {
            return Err(EconomyError::Capacity);
        }
        let canonical = ids.iter().copied().collect::<BTreeSet<_>>();
        if canonical.len() != ids.len() {
            return Err(EconomyError::OrderTooLarge);
        }
        let mut passengers = Vec::with_capacity(canonical.len());
        for &id in &canonical {
            let index = *self.lookup.get(&id).ok_or(EconomyError::UnknownEntity)?;
            let unit = self.units[index].state;
            if unit.player != player {
                return Err(EconomyError::NotOwner);
            }
            if unit.kind.is_naval() || unit.kind == UnitKind::Vassal {
                return Err(EconomyError::WrongUnitKind);
            }
            if unit.hp == 0 || self.embarked(id) {
                return Err(EconomyError::InvalidTarget);
            }
            passengers.push((id, index, unit.position));
        }
        if passengers
            .iter()
            .any(|(_, _, at)| !adjacent(at.tile_floor(), bank))
        {
            let (land, water) = self
                .boarding_shore(passengers[0].2.tile_floor(), bank)
                .ok_or(EconomyError::InvalidTarget)?;
            self.queue_transport_journey(
                transport,
                Journey {
                    player,
                    passengers: canonical.into_iter().collect(),
                    land,
                    water,
                    unloading: false,
                },
            );
            return Ok(());
        }
        for (id, index, at) in passengers {
            self.stop_general(id);
            if let Some(g) = self.general_controller(id) {
                self.restore_general_stance(g, id);
            }
            self.stop_unit_orders(index);
            self.cancel_hunt(id);
            self.move_queues.remove(&id);
            self.untrack_unit_at(id, at);
            self.update_observer(id, None);
            self.economy.embarked.insert(id, transport);
            self.units[index].state.task = UnitTask::Idle;
        }
        let cargo = self.economy.cargo.entry(transport).or_default();
        cargo.extend(canonical);
        cargo.sort_unstable();
        Ok(())
    }

    /// All passengers land on a validated adjacent dry tile. Soft spacing
    /// spreads the group on later ticks, just like other stacked units.
    pub fn issue_unload(
        &mut self,
        player: PlayerId,
        transport: EntityId,
        tile: TileCoord,
    ) -> Result<(), EconomyError> {
        let bank = self.validate_transport(player, transport)?;
        let at = WorldPosition::from_tile_center(tile).map_err(|_| EconomyError::InvalidTarget)?;
        if !self.config.valid_ground_position(at) || self.economy.blocked_tiles.contains(&tile) {
            return Err(EconomyError::InvalidTarget);
        }
        let water = self
            .unloading_shore(tile, bank)
            .ok_or(EconomyError::InvalidTarget)?;
        if !adjacent(bank, tile) {
            self.queue_transport_journey(
                transport,
                Journey {
                    player,
                    passengers: Vec::new(),
                    land: tile,
                    water,
                    unloading: true,
                },
            );
            return Ok(());
        }
        self.cancel_transport_journey(transport);
        let ids = self.economy.cargo.remove(&transport).unwrap_or_default();
        for id in ids {
            self.economy.embarked.remove(&id);
            let index = self.lookup[&id];
            let state = &mut self.units[index].state;
            state.position = at;
            state.previous_position = at;
            state.task = UnitTask::Idle;
            let unit = *state;
            if self.units[index].bucket != ChunkCoord::from_position(at) {
                self.move_bucket(index, ChunkCoord::from_position(at));
            }
            self.track_unit_cell(&unit);
            self.refresh_spacing_order(id);
            self.update_unit_vision(id);
        }
        Ok(())
    }

    pub(crate) fn sink_cargo(&mut self, transport: EntityId) {
        self.cancel_transport_journey(transport);
        let ids = self.economy.cargo.remove(&transport).unwrap_or_default();
        for id in ids {
            // Keep dead passengers hidden until their normal corpse removal.
            if let Some(&index) = self.lookup.get(&id) {
                self.kill_unit(index);
            }
        }
    }

    pub(crate) fn upgrade_galleys(&mut self, player: PlayerId, changed: &mut Vec<EntityId>) {
        for unit in &mut self.units {
            let u = &mut unit.state;
            if u.player == player && u.kind == UnitKind::Galley && u.hp > 0 {
                let bonus = unit_combat(UnitKind::WarGalley).hp - unit_combat(UnitKind::Galley).hp;
                u.kind = UnitKind::WarGalley;
                u.hp += bonus;
                u.max_hp += bonus;
                changed.push(u.id);
            }
        }
        for building in self
            .economy
            .buildings
            .values_mut()
            .filter(|b| b.player == player)
        {
            for entry in &mut building.training_queue {
                if entry.kind == UnitKind::Galley {
                    entry.kind = UnitKind::WarGalley;
                }
            }
        }
        let ids = self
            .units()
            .filter(|u| u.player == player && u.kind == UnitKind::WarGalley)
            .map(|u| u.id)
            .collect::<Vec<_>>();
        for id in ids {
            self.update_unit_vision(id);
        }
    }
}
