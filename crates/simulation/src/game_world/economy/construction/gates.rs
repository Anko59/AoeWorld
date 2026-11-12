//! A completed (palisade or stone) gate opens when an owned unit approaches, and cannot close
//! through a living unit. It is four by one tiles along a wall line in either axis.
//! Planning admits an unlocked owner gate, or an unlocked currently open enemy
//! gate. Collision opens on approach, and never closes through a living unit.
use crate::{GameWorld, Terrain};
use aoe_core::{TileCoord, TileRect};

impl GameWorld {
    /// Locking changes planning admission immediately. Physical closing waits
    /// for the ordinary gate tick, which keeps a living threshold occupant safe.
    pub fn set_gate_locked(
        &mut self,
        player: aoe_core::PlayerId,
        id: aoe_core::EntityId,
        locked: bool,
    ) -> Result<(), crate::EconomyError> {
        let gate = self
            .building_mut(id)
            .ok_or(crate::EconomyError::UnknownEntity)?;
        if gate.player != player {
            return Err(crate::EconomyError::NotOwner);
        }
        if !gate.kind.is_gate() {
            return Err(crate::EconomyError::NotBuildable);
        }
        if !gate.is_complete() {
            return Err(crate::EconomyError::UnderConstruction);
        }
        if gate.gate_locked == locked {
            return Ok(());
        }
        gate.gate_locked = locked;
        let rect = gate.footprint;
        if locked {
            self.collision_closed(rect);
        } else {
            self.collision_opened(rect);
        }
        self.remember_visible_buildings();
        Ok(())
    }
    /// A save puts a completed gate back: ground opens as it does on the first
    /// tick, and an open gate also frees its collision tiles.
    pub(crate) fn restore_gate(&mut self, id: aoe_core::EntityId, open: bool) {
        let Some(rect) = self.building(id).map(|b| b.footprint) else {
            return;
        };
        if let Terrain::Map { overlay, .. } = &mut self.terrain
            && overlay.blocks_tile(rect.min)
        {
            overlay.remove_footprint(rect);
            self.collision_opened(rect);
        }
        if !open {
            return;
        }
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                self.economy.blocked_tiles.remove(&TileCoord::new(x, y));
            }
        }
        self.economy.open_gates.insert(id);
        if let Some(gate) = self.building_mut(id) {
            gate.gate_open = true;
        }
        self.collision_opened(rect);
    }

    pub(super) fn advance_gates(&mut self) {
        // Only the gate index is visited; `Pass::AdvanceGates` counts the
        // whole-building scan this replaced and stays at 0.
        let gates: Vec<_> = self
            .economy
            .gates
            .iter()
            .filter_map(|id| self.economy.buildings.get(id))
            .filter(|b| b.is_complete())
            .map(|b| (b.id, b.player, b.footprint, b.gate_locked))
            .collect();
        for (id, owner, rect, locked) in gates {
            if let Terrain::Map { overlay, .. } = &mut self.terrain
                && overlay.blocks_tile(rect.min)
            {
                overlay.remove_footprint(rect);
                self.collision_opened(rect);
            }
            let query = TileRect::new(
                TileCoord::new((rect.min.x - 2).max(0), (rect.min.y - 2).max(0)),
                TileCoord::new(
                    (rect.max.x + 2).min(self.config.width_tiles),
                    (rect.max.y + 2).min(self.config.height_tiles),
                ),
            );
            let nearby = self.query(query).0;
            let open = (!locked && nearby.iter().any(|u| u.hp > 0 && u.player == owner))
                || nearby
                    .iter()
                    .any(|u| u.hp > 0 && rect.contains(u.position.tile_floor()));
            if open == self.economy.open_gates.contains(&id) {
                continue;
            }
            for y in rect.min.y..rect.max.y {
                for x in rect.min.x..rect.max.x {
                    if open {
                        self.economy.blocked_tiles.remove(&TileCoord::new(x, y));
                    } else {
                        self.economy.blocked_tiles.insert(TileCoord::new(x, y));
                    }
                }
            }
            if open {
                self.economy.open_gates.insert(id);
            } else {
                self.economy.open_gates.remove(&id);
            }
            if let Some(gate) = self.building_mut(id) {
                gate.gate_open = open;
            }
            if open {
                self.collision_opened(rect);
            } else {
                self.collision_closed(rect);
            }
        }
    }
}

impl GameWorld {
    /// Build a gate whose four tiles run along `y` when `vertical` (in a wall
    /// line of constant x), else along `x`, like [`GameWorld::issue_build`].
    pub fn issue_build_gate(
        &mut self,
        player: aoe_core::PlayerId,
        builders: &[aoe_core::EntityId],
        kind: crate::BuildingKind,
        origin: TileCoord,
        vertical: bool,
        queued: bool,
    ) -> Result<aoe_core::EntityId, crate::EconomyError> {
        if !kind.is_gate() || !vertical {
            return self.issue_build(player, builders, kind, origin, queued);
        }
        let builders = self.validate_builders(player, builders)?;
        self.validate_build_queue(&builders, 1, queued)?;
        let cost = self.check_buildable(player, kind, 1)?;
        let r = kind.rules();
        let footprint = TileRect::from_xywh(origin.x, origin.y, r.depth, r.side);
        self.validate_explored_footprint(player, footprint)?;
        self.validate_footprint_ground(footprint)?;
        let id = self.insert_building(player, kind, footprint, false)?;
        self.charge(player, cost);
        self.step_builders_aside(&builders, id);
        if !self.assign_build_orders(&builders, &[id], queued) {
            let _ = self.issue_cancel_construction(player, id);
            return Err(crate::GameWorldError::Unreachable.into());
        }
        self.remember_visible_buildings();
        Ok(id)
    }
}
