//! Living prey and birth deadlines for recreation from an immutable map package.
use super::{AnimalKind, HERD_RECRUIT_ID_START};
use crate::GameWorld;
use crate::game_world::snapshot::{AnimalRecord, HerdRecord};
use aoe_map::ResourceOverlayError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A complete replacement for the starting wildlife that can form herds.
/// An empty snapshot records extinction.
#[derive(Debug, Deserialize, Serialize)]
pub struct MapHerdSnapshot {
    animals: Vec<AnimalRecord>,
    herds: Vec<HerdRecord>,
    next_recruit: u32,
    tick_hz: u32,
}

impl MapHerdSnapshot {
    pub fn has_pending_births(&self) -> bool {
        self.herds.iter().any(|herd| herd.due.is_some())
    }

    pub fn tick_hz(&self) -> u32 {
        self.tick_hz
    }

    /// Apply elapsed game ticks from the checkpoint's clock record.
    pub fn elapse(&mut self, ticks: u64) -> Result<(), ResourceOverlayError> {
        for due in self.herds.iter_mut().filter_map(|herd| herd.due.as_mut()) {
            *due = due
                .checked_sub(ticks)
                .ok_or(ResourceOverlayError::InvalidSnapshot)?;
        }
        Ok(())
    }
}

impl GameWorld {
    pub fn herd_recovery_revision(&self) -> u64 {
        self.wildlife.recovery.revision
    }

    pub fn herd_recovery_pending(&self) -> bool {
        self.wildlife.recovery.has_pending_births()
    }

    pub fn map_herd_snapshot(&self) -> MapHerdSnapshot {
        let mut herds = self.herd_records();
        for herd in &mut herds {
            herd.due = herd.due.map(|due| due.saturating_sub(self.tick.0));
        }
        MapHerdSnapshot {
            animals: self.animal_records_of(|kind| kind != AnimalKind::Wolf),
            herds,
            next_recruit: self.wildlife.next_herd_recruit,
            tick_hz: self.config.tick_hz,
        }
    }

    /// Replaces freshly seeded prey. The resource overlay and village ground
    /// have already been restored, so survivors can be placed on current terrain.
    pub fn restore_map_herds(
        &mut self,
        checkpoint: MapHerdSnapshot,
    ) -> Result<(), ResourceOverlayError> {
        let invalid = ResourceOverlayError::InvalidSnapshot;
        let end = HERD_RECRUIT_ID_START
            .checked_add(checkpoint.next_recruit)
            .ok_or(invalid)?;
        if self.tick.0 != 0
            || checkpoint.tick_hz == 0
            || checkpoint.animals.len() > self.limits().animals
            || checkpoint.herds.len() > checkpoint.animals.len()
            || checkpoint
                .herds
                .windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
            || checkpoint
                .animals
                .windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
            || checkpoint.animals.iter().any(|animal| {
                animal.id.0 >= end
                    || animal.kind == AnimalKind::Wolf
                    || !(1..=animal.kind.rules().hp).contains(&animal.hp)
                    || animal.hp_quarters >= 4
                    || animal.food_remaining > animal.kind.rules().food
            })
        {
            return Err(invalid);
        }

        for kind in [AnimalKind::Sheep, AnimalKind::Deer, AnimalKind::Boar] {
            self.remove_animals(kind);
        }
        let mut ids = BTreeMap::new();
        for record in &checkpoint.animals {
            let id = self.restore_map_animal(record).ok_or(invalid)?;
            ids.insert(record.id, id);
        }
        self.wildlife.next_herd_recruit = checkpoint.next_recruit;

        let mut herds = checkpoint.herds;
        let remapped = ids.iter().any(|(old, new)| old != new);
        for herd in &mut herds {
            if herd.members.windows(2).any(|ids| ids[0] >= ids[1]) {
                return Err(invalid);
            }
            for member in &mut herd.members {
                if *member < herd.id {
                    return Err(invalid);
                }
                *member = *ids.get(member).ok_or(invalid)?;
            }
            herd.members.sort_unstable();
            if remapped {
                herd.id = *herd.members.first().ok_or(invalid)?;
            }
            herd.due = herd.due.map(|remaining| {
                let ticks = (u128::from(remaining) * u128::from(self.config.tick_hz))
                    .div_ceil(u128::from(checkpoint.tick_hz));
                u64::try_from(ticks).unwrap_or(u64::MAX)
            });
        }
        if !self.restore_herds(&herds) {
            return Err(invalid);
        }
        self.wildlife.recovery.revision = 1;
        Ok(())
    }
}
