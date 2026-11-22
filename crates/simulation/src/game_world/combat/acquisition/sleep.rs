//! Proven empty threat regions retain logical turns without polling units.
//! Only a unit of another player entering a sleeper's chunks can end that
//! proof, so a crossing or a spawn wakes the sleepers of other players only;
//! a skipped turn charges the buckets' sizes at that tick, as a scan would.
use super::super::targeting::{BUCKET_WINDOW, TargetArea};
use crate::{GameUnit, GameWorld};
use aoe_core::{ChunkCoord, EntityId, PlayerId, TileRect};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct Sleep {
    chunks: Vec<ChunkCoord>,
    needed: u32,
    player: PlayerId,
    #[cfg(test)]
    source: GameUnit,
}

#[derive(Debug, Default)]
pub(crate) struct Quiet {
    jobs: BTreeMap<EntityId, Sleep>,
    chunks: BTreeMap<ChunkCoord, BTreeSet<EntityId>>,
    parked: BTreeSet<EntityId>,
}

fn chunks(bounds: TileRect) -> impl Iterator<Item = ChunkCoord> {
    let size = aoe_core::SPATIAL_CHUNK_TILES;
    let first = ChunkCoord {
        x: bounds.min.x.div_euclid(size),
        y: bounds.min.y.div_euclid(size),
    };
    let last = ChunkCoord {
        x: (bounds.max.x - 1).div_euclid(size),
        y: (bounds.max.y - 1).div_euclid(size),
    };
    (first.y..=last.y).flat_map(move |y| (first.x..=last.x).map(move |x| ChunkCoord { x, y }))
}

impl Quiet {
    #[inline]
    pub(crate) fn wake(&mut self, id: EntityId) {
        if self.jobs.is_empty() && self.parked.is_empty() {
            return;
        }
        self.parked.remove(&id);
        let Some(job) = self.jobs.remove(&id) else {
            return;
        };
        for chunk in job.chunks {
            if let Some(ids) = self.chunks.get_mut(&chunk) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.chunks.remove(&chunk);
                }
            }
        }
    }

    pub(crate) fn wake_chunk(&mut self, chunk: ChunkCoord) {
        let Some(ids) = self.chunks.get(&chunk).cloned() else {
            return;
        };
        for id in ids {
            self.wake(id);
        }
    }

    /// `unit` enters `chunk`: it wakes itself, and the chunk's sleepers of
    /// every other player. Leaving a chunk cannot end a proof.
    pub(crate) fn arrive(&mut self, chunk: ChunkCoord, unit: &GameUnit) {
        self.wake(unit.id);
        self.wake_chunk_for(chunk, unit.player);
    }

    /// Wakes the chunk's sleepers that `arrival` is a threat to: those of
    /// every other player.
    pub(crate) fn wake_chunk_for(&mut self, chunk: ChunkCoord, arrival: PlayerId) {
        let Some(ids) = self.chunks.get(&chunk) else {
            return;
        };
        let woken: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| self.jobs.get(id).is_some_and(|job| job.player != arrival))
            .collect();
        for id in woken {
            self.wake(id);
        }
    }

    pub(crate) fn wake_area(&mut self, area: TileRect) {
        for chunk in chunks(area) {
            self.wake_chunk(chunk);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.jobs.clear();
        self.chunks.clear();
        self.parked.clear();
    }

    pub(super) fn park(&mut self, id: EntityId) {
        self.wake(id);
        self.parked.insert(id);
    }
}

impl GameWorld {
    /// What a scan of `chunks` charges to the shared target budget.
    fn bucket_scan(&self, chunks: &[ChunkCoord]) -> u32 {
        chunks
            .iter()
            .filter_map(|c| self.chunks.get(c))
            .map(|b| b.len().min(BUCKET_WINDOW) as u32)
            .sum()
    }

    fn quiet_region(&self, u: &GameUnit, area: &TargetArea) -> bool {
        let bounds = area.bounds(self);
        // A hidden or out-of-range enemy in a touched bucket keeps this query
        // awake. In particular, sampled dense buckets cannot hide a threat.
        for chunk in chunks(bounds) {
            if self.chunks.get(&chunk).is_some_and(|bucket| {
                bucket.iter().any(|id| {
                    self.unit(*id)
                        .is_some_and(|other| other.player != u.player && other.hp > 0)
                })
            }) {
                return false;
            }
        }
        for y in bounds.min.y..bounds.max.y {
            for x in bounds.min.x..bounds.max.x {
                if self
                    .building_at_tile(aoe_core::TileCoord::new(x, y))
                    .and_then(|id| self.economy.buildings.get(&id))
                    .is_some_and(|b| b.player != u.player)
                {
                    return false;
                }
            }
        }
        true
    }

    pub(in crate::game_world::combat) fn sleep_quiet_acquisition(
        &mut self,
        u: &GameUnit,
        area: &TargetArea,
    ) {
        if self.economy.no_sleep || !self.quiet_region(u, area) {
            return;
        }
        let chunks: Vec<_> = chunks(area.bounds(self)).collect();
        let needed = chunks.len() as u32 * BUCKET_WINDOW as u32;
        for chunk in &chunks {
            self.combat
                .quiet
                .chunks
                .entry(*chunk)
                .or_default()
                .insert(u.id);
        }
        self.combat.quiet.jobs.insert(
            u.id,
            Sleep {
                chunks,
                needed,
                player: u.player,
                #[cfg(test)]
                source: *u,
            },
        );
    }

    pub(in crate::game_world::combat) fn skip_quiet_acquisition(&mut self, id: EntityId) -> bool {
        if self.economy.no_sleep {
            self.combat.quiet.wake(id);
            return false;
        }
        if self.combat.quiet.parked.contains(&id) {
            self.combat.stats.acquisition_slept += 1;
            return true;
        }
        let Some(job) = self.combat.quiet.jobs.get(&id) else {
            return false;
        };
        // Preserve both the acquisition cursor and the shared targeting budget.
        // Otherwise sleeping a friendly army changes another army's first hit.
        let needed = job.needed;
        let scanned = self.bucket_scan(&job.chunks);
        if self.start_target_probe(id, needed).is_some() {
            self.combat.stats.targeting_scanned += scanned;
        }
        self.combat.stats.acquisition_slept += 1;
        true
    }

    #[cfg(test)]
    pub(in crate::game_world::combat) fn assert_quiet_acquisition(&self) {
        for &id in &self.combat.quiet.parked {
            let u = self.unit(id).expect("missed wake: inactive seeker removed");
            assert!(
                self.combat.seekers.contains(&id)
                    && (u.hp == 0
                        || u.auto_scout
                        || u.stance == aoe_core::CombatStance::NoAttack
                        || self.embarked(id)),
                "missed wake: inactive seeker {id:?}"
            );
        }
        for (&id, job) in &self.combat.quiet.jobs {
            let u = self.unit(id).expect("missed wake: removed seeker");
            let old = job.source;
            assert!(
                u.position == old.position
                    && u.player == old.player
                    && u.kind == old.kind
                    && u.stance == old.stance
                    && u.task == old.task
                    && u.moving == old.moving
                    && u.planning == old.planning
                    && u.auto_scout == old.auto_scout
                    && u.hp > 0
                    && !self.embarked(id),
                "missed wake: seeker {id:?}"
            );
            let area = self
                .acquisition_area(&u)
                .expect("missed wake: seeker observer");
            assert!(
                self.quiet_region(&u, &area),
                "missed wake: threat near {id:?}"
            );
            assert_eq!(job.player, u.player, "missed wake: seeker owner");
            assert_eq!(
                job.chunks,
                chunks(area.bounds(self)).collect::<Vec<_>>(),
                "missed wake: seeker range"
            );
        }
    }
}
