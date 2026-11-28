//! Fighters with nothing to fight leave the acquisition rotation (#978 4.1,
//! slice 2e). A sleeper proved its look holds no unit or building of another
//! player; only one of those entering its chunks can end that proof, so a
//! crossing or a spawn wakes the sleepers of other players only, and those
//! look on the next acquisition pass, before the rotation goes on. A fighter
//! that cannot act (no-attack stance, scouting, embarked) waits parked for the
//! order or stance change that wakes it. After a restore nobody sleeps: every
//! fighter takes its turn once and those with nothing near sleep again.
use super::super::targeting::TargetArea;
use crate::{GameUnit, GameWorld};
use aoe_core::{ChunkCoord, EntityId, PlayerId, TileRect};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct Sleep {
    chunks: Vec<ChunkCoord>,
    player: PlayerId,
    #[cfg(test)]
    source: GameUnit,
}

#[derive(Debug, Default)]
pub(crate) struct Quiet {
    jobs: BTreeMap<EntityId, Sleep>,
    chunks: BTreeMap<ChunkCoord, BTreeSet<EntityId>>,
    parked: BTreeSet<EntityId>,
    /// Seekers neither asleep nor parked: the acquisition rotation.
    pub(super) awake: super::awake::Awake,
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
    /// Asleep or parked: out of the rotation.
    #[cfg(test)]
    pub(crate) fn resting(&self, id: EntityId) -> bool {
        self.jobs.contains_key(&id) || self.parked.contains(&id)
    }

    /// A seeker joins the rotation, unless it is resting already.
    pub(crate) fn join(&mut self, id: EntityId) {
        if !self.jobs.contains_key(&id) && !self.parked.contains(&id) {
            self.awake.insert(id);
        }
    }

    /// A seeker that died, left the map or stopped seeking leaves for good.
    pub(crate) fn leave(&mut self, id: EntityId) {
        self.wake(id);
        self.awake.remove(&id);
    }

    #[inline]
    pub(crate) fn wake(&mut self, id: EntityId) {
        // Every event that wakes a seeker also ends its idle cadence: back in
        // the rotation first (as idle), then active.
        self.rouse(id);
        self.awake.activate(id);
    }

    /// A declaration of war invalidates the quiet proof even if nobody moved.
    pub(crate) fn rouse_player(&mut self, player: PlayerId) {
        let ids: Vec<_> = self
            .jobs
            .iter()
            .filter_map(|(&id, job)| (job.player == player).then_some(id))
            .collect();
        for id in ids {
            self.wake(id);
        }
    }

    /// Out of sleep or parking, still idle: an arrival near it changes
    /// nothing about the seeker itself.
    #[inline]
    pub(crate) fn rouse(&mut self, id: EntityId) {
        if self.jobs.is_empty() && self.parked.is_empty() {
            return;
        }
        if self.parked.remove(&id) {
            self.awake.insert(id);
        }
        let Some(job) = self.jobs.remove(&id) else {
            return;
        };
        self.awake.insert(id);
        for chunk in job.chunks {
            if let Some(ids) = self.chunks.get_mut(&chunk) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.chunks.remove(&chunk);
                }
            }
        }
    }

    /// Wakes a sleeper whose proof an arrival may have ended. A sleeper
    /// stands idle, so it looks on its next idle turn, within a second, like
    /// any idle soldier: a passing army does not make every sleeper near it
    /// look on the same tick.
    fn alarm(&mut self, id: EntityId) {
        if self.jobs.contains_key(&id) {
            self.rouse(id);
        }
    }

    pub(crate) fn wake_chunk(&mut self, chunk: ChunkCoord) {
        let Some(ids) = self.chunks.get(&chunk).cloned() else {
            return;
        };
        for id in ids {
            self.alarm(id);
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
            self.alarm(id);
        }
    }

    pub(crate) fn wake_area(&mut self, area: TileRect) {
        for chunk in chunks(area) {
            self.wake_chunk(chunk);
        }
    }

    /// Wakes everyone (sleep switched off, or a rule every proof read changed).
    pub(crate) fn clear(&mut self) {
        let asleep = self
            .jobs
            .keys()
            .chain(&self.parked)
            .copied()
            .collect::<Vec<_>>();
        self.awake.extend(asleep);
        self.jobs.clear();
        self.chunks.clear();
        self.parked.clear();
    }

    pub(super) fn park(&mut self, id: EntityId) {
        self.wake(id);
        self.parked.insert(id);
        self.awake.remove(&id);
    }
}

impl GameWorld {
    /// Any unit or building of another player in a touched chunk, hidden or
    /// out of range, keeps this query awake; sampled dense buckets cannot hide
    /// a threat. Read from the chunk summaries, never the units or tiles.
    fn quiet_region(&self, u: &GameUnit, area: &TargetArea) -> bool {
        chunks(area.bounds(self)).all(|chunk| {
            !self.enemies_present(&self.chunks.presence, chunk, u.player)
                && !self.enemies_present(&self.vision.building_chunks, chunk, u.player)
        })
    }

    pub(in crate::game_world::combat) fn sleep_quiet_acquisition(
        &mut self,
        u: &GameUnit,
        area: &TargetArea,
    ) {
        if !self.quiet_region(u, area) {
            return;
        }
        let chunks: Vec<_> = chunks(area.bounds(self)).collect();
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
                player: u.player,
                #[cfg(test)]
                source: *u,
            },
        );
        self.combat.quiet.awake.remove(&u.id);
    }

    /// A fighter that enters the world (a spawn, a trained unit) with no other
    /// player anywhere near starts asleep: its first look could only prove
    /// that again. An arrival near it wakes it as it would any sleeper.
    pub(crate) fn sleep_new_seeker(&mut self, id: EntityId) {
        let Some(u) = self.unit(id) else {
            return;
        };
        if !super::autonomous_kind(u.kind)
            || u.hp == 0
            || u.auto_scout
            || u.stance == aoe_core::CombatStance::NoAttack
            || u.moving
            || u.planning
            || !matches!(u.task, crate::UnitTask::Idle)
            || self.embarked(id)
            || !self.combat.quiet.awake.contains(&id)
        {
            return;
        }
        if let Some(area) = self.acquisition_area(&u) {
            self.sleep_quiet_acquisition(&u, &area);
        }
    }

    /// Fighters asleep or parked, out of the rotation.
    pub(in crate::game_world::combat) fn resting_seekers(&self) -> u32 {
        (self.combat.quiet.jobs.len() + self.combat.quiet.parked.len()) as u32
    }

    #[cfg(test)]
    pub(in crate::game_world::combat) fn assert_quiet_acquisition(&self) {
        let quiet = &self.combat.quiet;
        let mut all = quiet.awake.set();
        for id in quiet.jobs.keys().chain(&quiet.parked) {
            assert!(all.insert(*id), "missed wake: {id:?} asleep and awake");
        }
        assert_eq!(
            all, self.combat.seekers,
            "missed wake: rotation and seekers"
        );
        let mut units = crate::game_world::unit_index::ChunkPresence::default();
        for stored in self.units.iter() {
            if let Some(player) = stored.living_owner() {
                units.add(stored.bucket, player);
            }
        }
        assert_eq!(
            units.normalized(),
            self.chunks.presence.normalized(),
            "missed wake: unit chunk summary"
        );
        assert_eq!(
            self.building_chunks_recount().normalized(),
            self.vision.building_chunks.normalized(),
            "missed wake: building chunk summary"
        );
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

#[cfg(test)]
mod tests {
    use super::Quiet;
    use aoe_core::EntityId;

    #[test]
    fn a_parked_seeker_woken_by_an_event_looks_on_the_active_turn() {
        let mut quiet = Quiet::default();
        let id = EntityId(7);
        quiet.join(id);
        quiet.park(id);
        quiet.wake(id);
        assert!(quiet.awake.is_active(id));
    }
}
