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
        // The saved game held these animals already: its limit is not applied again.
        let limit = std::mem::replace(&mut self.economy.limits.animals, usize::MAX);
        let mut spawned = self.spawn_animal(record.kind, record.position);
        // A foundation laid over it since: the nearest open tile, ring by ring.
        let home = record.position.tile_floor();
        for radius in 1..=12_i32 {
            if spawned.is_ok() {
                break;
            }
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if spawned.is_ok() || (dx.abs() != radius && dy.abs() != radius) {
                        continue;
                    }
                    let tile = aoe_core::TileCoord::new(home.x + dx, home.y + dy);
                    if let Ok(at) = WorldPosition::from_tile_center(tile) {
                        spawned = self.spawn_animal(record.kind, at);
                    }
                }
            }
        }
        self.economy.limits.animals = limit;
        let Ok(id) = spawned else {
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
        if !self.config.valid_ground_position(at)
            || !self.tile_passable(at.tile_floor())
            || (kind == AnimalKind::Wolf && !self.animal_spawn_open(at.tile_floor()))
        {
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
    pub(crate) fn initialize_wildlife(&mut self, center: TileCoord, root: TileCoord) {
        let region = self.animal_start_region(center, root);
        let mut used = std::collections::BTreeSet::new();
        let extra_boar = self.start_roll(center, 200).is_multiple_of(2);
        for (slot, (kind, count, min, max)) in [
            (
                AnimalKind::Sheep,
                if extra_boar { 7 } else { 8 },
                3_u64,
                6_u64,
            ),
            (AnimalKind::Boar, if extra_boar { 3 } else { 2 }, 9, 15),
            (AnimalKind::Deer, if extra_boar { 2 } else { 4 }, 9, 15),
            (AnimalKind::Wolf, 1, 16, 20),
        ]
        .into_iter()
        .enumerate()
        {
            let distance = |t: TileCoord, c: TileCoord| {
                u64::from(t.x.abs_diff(c.x)).pow(2) + u64::from(t.y.abs_diff(c.y)).pow(2)
            };
            let mut candidates: Vec<_> = region
                .iter()
                .copied()
                .filter(|tile| {
                    (min.pow(2)..=max.pow(2)).contains(&distance(*tile, center))
                        && self.tile_free(*tile)
                        && (kind != AnimalKind::Wolf || self.animal_spawn_open(*tile))
                })
                .collect();
            let salt = self.start_roll(center, 210 + slot as u64);
            candidates.sort_by_key(|tile| self.start_roll(*tile, salt));
            let Some(anchor) = candidates.first().copied() else {
                continue;
            };
            candidates.sort_by_key(|tile| {
                distance(*tile, anchor) * 8 + self.start_roll(*tile, salt) % 64
            });
            for tile in candidates
                .into_iter()
                .filter(|tile| !used.contains(tile))
                .take(count)
                .collect::<Vec<_>>()
            {
                if let Ok(at) = WorldPosition::from_tile_center(tile)
                    && self.spawn_animal(kind, at).is_ok()
                {
                    used.insert(tile);
                }
            }
        }
    }
}
