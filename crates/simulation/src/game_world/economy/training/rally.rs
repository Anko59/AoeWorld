/// A production building's destination for newly trained units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RallyTarget {
    Ground(aoe_core::WorldPosition),
    Resource(u64),
    Building(aoe_core::EntityId),
    Animal(aoe_core::EntityId),
}

/// Last accepted location remains available if the target disappears.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RallyPoint {
    pub target: RallyTarget,
    pub position: aoe_core::WorldPosition,
}

use super::super::{
    EconomyError, UnitKind,
    placement::{distance_squared, perimeter, position},
};
use crate::{GameWorld, GameWorldError, Terrain, TileVisibility};
use aoe_core::{EntityId, PlayerId};

impl GameWorld {
    /// Validates ownership and current sight before replacing the old rally.
    pub fn issue_set_rally(
        &mut self,
        player: PlayerId,
        id: EntityId,
        target: RallyTarget,
    ) -> Result<(), EconomyError> {
        let building = self.building(id).ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if super::super::trainable_units(building.kind).is_empty() {
            return Err(EconomyError::WrongUnitKind);
        }
        let point = match target {
            RallyTarget::Ground(p) => {
                if !self.config.valid_map_position(p) {
                    return Err(GameWorldError::InvalidPosition.into());
                }
                self.config.snap_ground_position(p)
            }
            RallyTarget::Resource(id) => {
                let Terrain::Map { generator, overlay } = &self.terrain else {
                    return Err(EconomyError::InvalidResource);
                };
                let node = overlay
                    .resource_by_id_with_cancel(generator, id, &|| false)
                    .map_err(GameWorldError::Environment)?
                    .filter(|n| overlay.resource_available(*n))
                    .ok_or(EconomyError::InvalidResource)?;
                if self.visibility(player, node.tile) == TileVisibility::Unexplored {
                    return Err(EconomyError::InvalidResource);
                }
                position(node.tile)?
            }
            RallyTarget::Building(id) => {
                let b = self
                    .building(id)
                    .filter(|b| b.player == player)
                    .ok_or(EconomyError::InvalidTarget)?;
                position(aoe_core::TileCoord::new(
                    (b.footprint.min.x + b.footprint.max.x) / 2,
                    (b.footprint.min.y + b.footprint.max.y) / 2,
                ))?
            }
            RallyTarget::Animal(id) => {
                let a = self.huntable(player, id)?;
                if a.food_remaining == 0 {
                    return Err(EconomyError::InvalidResource);
                }
                if a.owner != Some(player)
                    && self.visibility(player, a.position.tile_floor()) != TileVisibility::Visible
                {
                    return Err(EconomyError::InvalidTarget);
                }
                a.position
            }
        };
        let target = if matches!(target, RallyTarget::Ground(_)) {
            RallyTarget::Ground(point)
        } else {
            target
        };
        self.economy
            .buildings
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?
            .rally_point = Some(RallyPoint {
            target,
            position: point,
        });
        Ok(())
    }

    pub(super) fn apply_rally(&mut self, id: EntityId, rally: RallyPoint) {
        let Some(unit) = self.unit(id) else {
            return;
        };
        let assigned = match rally.target {
            RallyTarget::Resource(node)
                if matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip) =>
            {
                self.issue_gather(unit.player, id, node).is_ok()
            }
            RallyTarget::Animal(animal) if unit.kind == UnitKind::Villager => {
                self.issue_hunt(unit.player, &[id], animal).is_ok()
            }
            RallyTarget::Building(target) if unit.kind == UnitKind::Villager => {
                match self.building(target).filter(|b| b.player == unit.player) {
                    Some(b) if !b.is_complete() => {
                        self.issue_construct(unit.player, &[id], target).is_ok()
                    }
                    Some(b) if b.kind == super::super::BuildingKind::Farm => {
                        self.issue_farm(unit.player, id, target).is_ok()
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if assigned {
            return;
        }
        // Military units and workers unable to work this target still walk
        // beside the object, rather than trying to stand inside its blocker.
        if matches!(
            rally.target,
            RallyTarget::Resource(_) | RallyTarget::Animal(_)
        ) && self
            .approach_resource(id, rally.position.tile_floor())
            .is_ok()
        {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].state.task = crate::UnitTask::Moving;
            }
            return;
        }
        let destination = match rally.target {
            RallyTarget::Building(target) => self
                .building(target)
                .filter(|b| b.player == unit.player)
                .and_then(|b| {
                    let mut tiles = perimeter(b.footprint);
                    tiles.sort_by_key(|t| {
                        (distance_squared(unit.position.tile_floor(), *t), t.y, t.x)
                    });
                    tiles.into_iter().find(|t| {
                        self.unit_tile_passable(unit.kind, *t, &|| false)
                            .unwrap_or(false)
                    })
                })
                .and_then(|t| position(t).ok())
                .unwrap_or(rally.position),
            _ => rally.position,
        };
        let _ = self.issue_move(id, destination);
    }
}

pub(crate) fn hash_rally(hash: &mut blake3::Hasher, rally: Option<RallyPoint>) {
    let Some(r) = rally else {
        hash.update(&[0]);
        return;
    };
    hash.update(&[1]);
    hash.update(&r.position.x.to_le_bytes());
    hash.update(&r.position.y.to_le_bytes());
    let (tag, id) = match r.target {
        RallyTarget::Ground(_) => (0, 0),
        RallyTarget::Resource(id) => (1, id),
        RallyTarget::Building(id) => (2, u64::from(id.0)),
        RallyTarget::Animal(id) => (3, u64::from(id.0)),
    };
    hash.update(&[tag]);
    hash.update(&id.to_le_bytes());
}
