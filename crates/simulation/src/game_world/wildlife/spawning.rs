use super::{AnimalKind, AnimalTask, GameAnimal, StoredAnimal, cell};
use crate::{EconomyError, GameWorld};
use aoe_core::{EntityId, TileCoord, WorldPosition};

impl GameWorld {
    /// Living animals as a save keeps them (carcasses are not kept).
    pub(crate) fn animal_records(&self) -> Vec<crate::game_world::snapshot::AnimalRecord> {
        self.wildlife
            .animals
            .values()
            .filter(|a| a.state.hp > 0 && a.state.death_tick.is_none())
            .map(|a| crate::game_world::snapshot::AnimalRecord {
                id: a.state.id,
                kind: a.state.kind,
                owner: a.state.owner,
                hp: a.state.hp,
                position: a.state.position,
                food_remaining: a.state.food_remaining,
                facing: a.state.facing,
                home: a.home,
            })
            .collect()
    }

    /// Puts a saved animal back at its own identity (the next allocation).
    pub(crate) fn restore_animal(
        &mut self,
        record: &crate::game_world::snapshot::AnimalRecord,
    ) -> bool {
        let Ok(id) = self.spawn_animal(record.kind, record.position) else {
            return false;
        };
        if id != record.id {
            return false;
        }
        if let Some(animal) = self.wildlife.animals.get_mut(&id) {
            animal.state.owner = record.owner;
            animal.state.hp = record.hp;
            animal.state.food_remaining = record.food_remaining;
            animal.state.facing = record.facing;
            animal.home = record.home;
        }
        true
    }
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

    /// Fixture hook: drop every living animal of one kind from the village kit.
    pub(crate) fn remove_animals(&mut self, kind: AnimalKind) {
        let found: Vec<_> = self
            .wildlife
            .animals
            .iter()
            .filter(|(_, a)| a.state.kind == kind)
            .map(|(id, a)| (*id, cell(a.state.position)))
            .collect();
        for (id, key) in found {
            self.wildlife.animals.remove(&id);
            self.wildlife.engaged.remove(&id);
            self.unindex_animal(id, key);
        }
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
