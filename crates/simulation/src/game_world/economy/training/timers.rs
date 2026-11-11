//! Queue clocks use the last completed phase, preserving reads before training.
use crate::{GameBuilding, GameWorld};
use aoe_core::{EntityId, PlayerId, TileCoord};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

mod events;
mod scheduling;

#[derive(Clone, Copy, Debug)]
pub(super) struct Clock {
    since: u64,
    until: Option<u64>,
    running: bool,
    #[cfg(test)]
    input: (u8, u64, u64),
    population: Option<bool>,
}

#[derive(Debug, Default)]
pub(crate) struct WorkClock {
    pub(crate) awake: BTreeSet<EntityId>,
    clocks: BTreeMap<EntityId, Clock>,
    due: BTreeSet<(u64, EntityId)>,
    pub(crate) tick: u64,
}

impl WorkClock {
    fn remove(&mut self, id: EntityId) {
        if let Some(clock) = self.clocks.remove(&id)
            && let Some(until) = clock.until
        {
            self.due.remove(&(until, id));
        }
    }

    fn insert(&mut self, id: EntityId, clock: Clock) {
        self.remove(id);
        self.awake.remove(&id);
        if let Some(until) = clock.until {
            self.due.insert((until, id));
        }
        self.clocks.insert(id, clock);
    }

    fn elapsed(&self, id: EntityId, now: u64) -> u64 {
        self.clocks.get(&id).map_or(0, |clock| {
            if clock.running {
                now.saturating_sub(clock.since)
            } else {
                0
            }
        })
    }

    fn progressing(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.clocks
            .iter()
            .filter(|(_, clock)| clock.running)
            .map(|(id, _)| *id)
    }
}

#[derive(Debug, Default)]
pub(crate) struct QueueTimers {
    pub(crate) training: WorkClock,
    pub(crate) research: WorkClock,
    /// During training, lower ids have already received this tick's credit.
    pub(crate) cursor: Option<EntityId>,
    players: BTreeMap<PlayerId, BTreeSet<EntityId>>,
    exits: BTreeMap<TileCoord, BTreeSet<EntityId>>,
    subscribed: BTreeMap<EntityId, Vec<TileCoord>>,
}

impl QueueTimers {
    pub(crate) fn register_training(&mut self, id: EntityId, player: PlayerId) {
        self.players.entry(player).or_default().insert(id);
    }

    pub(crate) fn unregister_training(&mut self, id: EntityId, player: PlayerId) {
        if let Some(ids) = self.players.get_mut(&player) {
            ids.remove(&id);
            if ids.is_empty() {
                self.players.remove(&player);
            }
        }
    }

    pub(crate) fn project<'a>(&self, mut b: Cow<'a, GameBuilding>) -> Cow<'a, GameBuilding> {
        let train = self.training.elapsed(b.id, self.training.tick);
        let research = self.research.elapsed(b.id, self.research.tick);
        if train > 0
            && let Some(entry) = b.to_mut().training_queue.front_mut()
        {
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(train)
                .min(entry.total_ticks);
        }
        if research > 0
            && let Some(entry) = &mut b.to_mut().research
        {
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(research)
                .min(entry.total_ticks);
        }
        b
    }

    pub(crate) fn progressing(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.training
            .progressing()
            .chain(self.research.progressing())
    }

    fn unsubscribe(&mut self, id: EntityId) {
        if let Some(tiles) = self.subscribed.remove(&id) {
            for tile in tiles {
                if let Some(ids) = self.exits.get_mut(&tile) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.exits.remove(&tile);
                    }
                }
            }
        }
    }
}

impl GameWorld {
    pub(crate) fn training_has_population(&self, player: aoe_core::PlayerId) -> bool {
        self.economy
            .players
            .get(&player)
            .is_some_and(|p| p.population_used < p.population_cap)
    }
}
