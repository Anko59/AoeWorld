//! A birth is due only while an original herd has both a vacancy and a survivor.
use super::{AnimalKind, GameAnimal, cell_of};
use crate::GameWorld;
use crate::game_world::snapshot::HerdRecord;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::collections::{BTreeMap, BTreeSet};

fn birth_seconds(kind: AnimalKind) -> Option<u64> {
    match kind {
        AnimalKind::Sheep => Some(300),
        AnimalKind::Deer => Some(240),
        AnimalKind::Boar => Some(360),
        AnimalKind::Wolf => None,
    }
}

#[derive(Debug, Default)]
pub(super) struct HerdRecovery {
    /// Dirty marker for map-only checkpoints, separate from gameplay state.
    pub(super) revision: u64,
    herds: BTreeMap<EntityId, Herd>,
    membership: BTreeMap<EntityId, EntityId>,
    due: BTreeSet<(u64, EntityId)>,
}

#[derive(Debug)]
struct Herd {
    kind: AnimalKind,
    capacity: u16,
    members: BTreeSet<EntityId>,
    due: Option<u64>,
}

impl HerdRecovery {
    pub(super) fn has_pending_births(&self) -> bool {
        !self.due.is_empty()
    }

    fn insert(&mut self, id: EntityId, herd: Herd) {
        self.membership
            .extend(herd.members.iter().map(|&member| (member, id)));
        if let Some(due) = herd.due {
            self.due.insert((due, id));
        }
        self.herds.insert(id, herd);
    }

    fn schedule(&mut self, id: EntityId, due: u64) {
        if let Some(herd) = self.herds.get_mut(&id)
            && herd.due.is_none()
        {
            herd.due = Some(due);
            self.due.insert((due, id));
        }
    }

    pub(super) fn update_hash(&self, hash: &mut blake3::Hasher) {
        if !self.herds.is_empty() {
            hash.update(b"herd recovery\0");
            hash.update(&(self.herds.len() as u64).to_le_bytes());
            for (&id, herd) in &self.herds {
                hash.update(&id.0.to_le_bytes());
                hash.update(&[herd.kind as u8]);
                hash.update(&herd.capacity.to_le_bytes());
                hash.update(&herd.due.unwrap_or(u64::MAX).to_le_bytes());
                hash.update(&(herd.members.len() as u16).to_le_bytes());
                for member in &herd.members {
                    hash.update(&member.0.to_le_bytes());
                }
            }
        }
    }
}

impl GameWorld {
    pub(crate) fn register_herd(&mut self, kind: AnimalKind, members: &[EntityId]) {
        if members.len() < 2 || birth_seconds(kind).is_none() {
            return;
        }
        let members = BTreeSet::from_iter(members.iter().copied());
        let Some(&id) = members.first() else {
            return;
        };
        self.wildlife.recovery.insert(
            id,
            Herd {
                kind,
                capacity: members.len() as u16,
                members,
                due: None,
            },
        );
        self.wildlife.recovery.revision += 1;
    }

    pub(super) fn herd_lost_member(&mut self, member: EntityId) {
        let recovery = &mut self.wildlife.recovery;
        let Some(id) = recovery.membership.remove(&member) else {
            return;
        };
        let Some(herd) = recovery.herds.get_mut(&id) else {
            return;
        };
        herd.members.remove(&member);
        if herd.members.is_empty() {
            if let Some(due) = herd.due {
                recovery.due.remove(&(due, id));
            }
            recovery.herds.remove(&id);
        } else if let Some(seconds) = birth_seconds(herd.kind) {
            recovery.schedule(
                id,
                self.tick
                    .0
                    .saturating_add(seconds * u64::from(self.config.tick_hz)),
            );
        }
    }

    pub(crate) fn advance_herd_recovery(&mut self) {
        while let Some(&(due, id)) = self
            .wildlife
            .recovery
            .due
            .first()
            .filter(|(due, _)| *due <= self.tick.0)
        {
            let recovery = &mut self.wildlife.recovery;
            recovery.due.remove(&(due, id));
            recovery.revision += 1;
            let Some(herd) = recovery.herds.get_mut(&id) else {
                continue;
            };
            herd.due = None;
            let kind = herd.kind;
            let Some(seconds) = birth_seconds(kind) else {
                continue;
            };
            let site = self.wildlife.recovery.herds[&id]
                .members
                .iter()
                .filter_map(|&member| self.animal(member))
                .find_map(|parent| self.birth_site(parent));
            if let Some((at, parent)) = site
                && let Ok(born) = self.spawn_herd_recruit(kind, at)
                && let Some(animal) = self.wildlife.animals.get_mut(&born)
            {
                animal.home = parent.home;
                animal.state.owner = parent.state.owner;
                if let Some(owner) = parent.state.owner {
                    self.update_observer(
                        born,
                        Some((owner, at.tile_floor(), kind.rules().los_tiles)),
                    );
                }
                let recovery = &mut self.wildlife.recovery;
                if let Some(herd) = recovery.herds.get_mut(&id) {
                    herd.members.insert(born);
                    recovery.membership.insert(born, id);
                }
            }
            let recovery = &mut self.wildlife.recovery;
            let herd = &recovery.herds[&id];
            if herd.members.len() < usize::from(herd.capacity) {
                let delay = seconds * u64::from(self.config.tick_hz);
                recovery.schedule(id, self.tick.0.saturating_add(delay));
            }
        }
    }

    fn birth_site(&self, parent: &GameAnimal) -> Option<(WorldPosition, super::StoredAnimal)> {
        let tile = parent.position.tile_floor();
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if self.animal_edge_open(tile, next)
                && !self.living_animal_at(next)
                && self.tile_free(next)
                && !self.tile_has_reservation(next)
                && !self.destination_tile_reserved(next)
                && let Ok(at) = WorldPosition::from_tile_center(next)
            {
                return Some((at, self.wildlife.animals[&parent.id]));
            }
        }
        None
    }

    pub(crate) fn living_animal_at(&self, tile: TileCoord) -> bool {
        self.wildlife.cells.get(&cell_of(tile)).is_some_and(|ids| {
            ids.iter()
                .filter_map(|&id| self.animal(id))
                .any(|a| a.hp > 0 && a.position.tile_floor() == tile)
        })
    }

    pub(crate) fn herd_records(&self) -> Vec<HerdRecord> {
        self.wildlife
            .recovery
            .herds
            .iter()
            .map(|(&id, herd)| HerdRecord {
                id,
                kind: herd.kind,
                capacity: herd.capacity,
                members: herd.members.iter().copied().collect(),
                due: herd.due,
            })
            .collect()
    }

    pub(crate) fn restore_herds(&mut self, records: &[HerdRecord]) -> bool {
        self.wildlife.recovery = HerdRecovery::default();
        for record in records {
            let Some(seconds) = birth_seconds(record.kind) else {
                return false;
            };
            let recovery = &self.wildlife.recovery;
            let capacity = usize::from(record.capacity);
            if capacity < 2
                || record.members.is_empty()
                || record.members.len() > capacity
                || (record.members.len() < capacity) != record.due.is_some()
                || recovery.herds.contains_key(&record.id)
                || record.members.windows(2).any(|ids| ids[0] >= ids[1])
                || record.members.iter().any(|id| {
                    *id < record.id
                        || recovery.membership.contains_key(id)
                        || self
                            .animal(*id)
                            .is_some_and(|a| a.hp == 0 || a.kind != record.kind)
                })
            {
                return false;
            }
            // An animal that could not be placed while restoring the map does
            // not seed recovery. Surviving herd mates can fill its place.
            let members = record
                .members
                .iter()
                .copied()
                .filter(|&id| self.animal(id).is_some())
                .collect::<BTreeSet<_>>();
            if members.is_empty() {
                continue;
            }
            let due = (members.len() < capacity).then(|| {
                record.due.unwrap_or_else(|| {
                    self.tick
                        .0
                        .saturating_add(seconds * u64::from(self.config.tick_hz))
                })
            });
            self.wildlife.recovery.insert(
                record.id,
                Herd {
                    kind: record.kind,
                    capacity: record.capacity,
                    members,
                    due,
                },
            );
        }
        true
    }
}
