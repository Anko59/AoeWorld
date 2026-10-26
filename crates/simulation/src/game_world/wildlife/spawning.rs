use super::{AnimalKind, AnimalTask, GameAnimal, StoredAnimal, cell};
use crate::{EconomyError, GameWorld};
use aoe_core::{EntityId, TileCoord, WorldPosition};

impl GameWorld {
    pub fn spawn_animal(
        &mut self,
        kind: AnimalKind,
        at: WorldPosition,
    ) -> Result<EntityId, EconomyError> {
        if self.wildlife.animals.len() >= self.economy.limits.animals {
            return Err(EconomyError::Capacity);
        }
        if !self.config.valid_ground_position(at) || !self.tile_passable(at.tile_floor()) {
            return Err(crate::GameWorldError::InvalidPosition.into());
        }
        let id = self.allocate_entity_id()?;
        self.wildlife.animals.insert(
            id,
            StoredAnimal {
                state: GameAnimal {
                    id,
                    kind,
                    owner: None,
                    hp: kind.rules().hp,
                    position: at,
                    task: AnimalTask::Idle,
                    food_remaining: kind.rules().food,
                    target: None,
                    facing: crate::Facing::South,
                    moving: false,
                    death_tick: None,
                },
                home: at,
                cooldown: 0,
                speed_carry: 0,
                decay_carry: 0,
                herd: None,
            },
        );
        self.wildlife.cells.entry(cell(at)).or_default().insert(id);
        Ok(id)
    }

    /// The terrain generator has no animal hooks. A bounded local kit is
    /// placed after the resource kit without changing terrain or collision.
    pub(crate) fn initialize_wildlife(&mut self, center: TileCoord) {
        let mut used = std::collections::BTreeSet::new();
        for (kind, count, offset) in [
            (AnimalKind::Sheep, 8, (-4, 0)),
            (AnimalKind::Boar, 2, (9, 3)),
            (AnimalKind::Deer, 4, (-8, 8)),
            (AnimalKind::Wolf, 1, (16, 0)),
        ] {
            for _ in 0..count {
                let mut chosen = None;
                'search: for ring in 0_i32..=4 {
                    for dy in -ring..=ring {
                        for dx in -ring..=ring {
                            if dx.abs().max(dy.abs()) != ring {
                                continue;
                            }
                            let tile =
                                TileCoord::new(center.x + offset.0 + dx, center.y + offset.1 + dy);
                            if !used.contains(&tile) && self.tile_free(tile) {
                                chosen = WorldPosition::from_tile_center(tile).ok();
                                break 'search;
                            }
                        }
                    }
                }
                if let Some(at) = chosen
                    && self.spawn_animal(kind, at).is_ok()
                {
                    used.insert(at.tile_floor());
                }
            }
        }
    }
}
