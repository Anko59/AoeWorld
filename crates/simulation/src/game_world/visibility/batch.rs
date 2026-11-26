//! Observer changes made during a movement tick or as fog returns, applied per
//! player in parallel. A player's sight is touched only by its own changes, so
//! each player's changes run in order on its own `PlayerVision` as one task.
//! That task also decides its building-watch events, which read only its own
//! sight; the shared watcher sets are then written in the same per-player
//! order. Players touch disjoint memberships there, so the result equals the
//! sequential replay for any number of threads.
use super::PlayerVision;
use super::change::{Change, Scratch};
use super::resources::observe_tiles;
use super::watchers::{Watch, WatchIndex};
use crate::GameWorld;
use crate::Terrain;
use crate::game_world::parallel::par_each_mut;
use aoe_core::PlayerId;
use std::collections::BTreeMap;

/// Changes that returning fog queues before it applies them.
pub(super) const FOG_SLICE: usize = 65_536;

struct Job {
    player: PlayerId,
    vision: PlayerVision,
    changes: Vec<Change>,
    watches: Vec<Watch>,
    scratch: Scratch,
}

impl Job {
    fn run(
        &mut self,
        size: (i32, i32),
        overlay: Option<&aoe_map::ResourceOverlay>,
        index: &WatchIndex,
    ) {
        for change in &self.changes {
            self.vision.apply_change(*change, size, &mut self.scratch);
            self.vision
                .sight_events(self.player, &self.scratch.flips, index, &mut self.watches);
            if let Some(overlay) = overlay {
                observe_tiles(overlay, &mut self.vision.resources, &self.scratch.entered);
            }
        }
    }
}

impl GameWorld {
    /// Starts queueing observer changes instead of applying their tile work.
    pub(crate) fn begin_vision_batch(&mut self) {
        self.vision.batch = Some(Vec::new());
    }

    /// Applies the queued changes, one player per parallel task.
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn flush_vision_batch(&mut self) {
        let Some(changes) = self.vision.batch.take() else {
            return;
        };
        self.apply_vision_changes(changes);
    }

    /// The disc of every unit, building and living owned animal, in that order,
    /// when fog returns. Slices of `slice` changes bound the queue whatever the
    /// population. They skip `flush_vision_batch`, which is a measured stage of
    /// the tick.
    pub(super) fn observe_all(&mut self, slice: usize) {
        self.begin_vision_batch();
        let units = self.units().map(|u| u.id).collect::<Vec<_>>();
        let buildings = self.buildings().map(|b| b.id).collect::<Vec<_>>();
        for id in units {
            self.update_unit_vision(id);
            self.apply_full_slice(slice);
        }
        for id in buildings {
            self.update_building_vision(id);
            self.apply_full_slice(slice);
        }
        let animals = self
            .animals()
            .filter(|a| a.hp > 0)
            .filter_map(|a| {
                a.owner
                    .map(|p| (a.id, p, a.position.tile_floor(), a.kind.rules().los_tiles))
            })
            .collect::<Vec<_>>();
        for (id, p, tile, los) in animals {
            self.update_observer(id, Some((p, tile, los)));
            self.apply_full_slice(slice);
        }
        // Closes the batch; a full last slice leaves nothing to apply.
        if let Some(rest) = self.vision.batch.take().filter(|rest| !rest.is_empty()) {
            self.apply_vision_changes(rest);
        }
    }

    /// Applies the queue once it holds `slice` changes; the batch stays open.
    fn apply_full_slice(&mut self, slice: usize) {
        let queue = self.vision.batch.as_mut();
        if let Some(full) = queue.filter(|queue| queue.len() >= slice) {
            let changes = std::mem::take(full);
            self.apply_vision_changes(changes);
        }
    }

    /// Queues the changes while a batch is open, otherwise applies them now.
    pub(super) fn submit_vision_changes(&mut self, changes: Vec<Change>) {
        match self.vision.batch.as_mut() {
            Some(batch) => batch.extend(changes),
            None => self.apply_vision_changes(changes),
        }
    }

    fn apply_vision_changes(&mut self, changes: Vec<Change>) {
        self.vision.applications += 1;
        let mut jobs: Vec<Job> = Vec::new();
        let mut slot_of = BTreeMap::new();
        for change in changes {
            let player = change.player();
            let slot = *slot_of.entry(player).or_insert_with(|| {
                jobs.push(Job {
                    player,
                    vision: self.vision.players.remove(&player).unwrap_or_default(),
                    changes: Vec::new(),
                    watches: Vec::new(),
                    scratch: Scratch::default(),
                });
                jobs.len() - 1
            });
            jobs[slot].changes.push(change);
        }
        let size = (self.config.width_tiles, self.config.height_tiles);
        let overlay = match &self.terrain {
            Terrain::Map { overlay, .. } => Some(overlay),
            _ => None,
        };
        let index = WatchIndex {
            building_at_tile: &self.vision.building_at_tile,
            watched: &self.vision.building_watchers,
        };
        if jobs.len() > 1 {
            par_each_mut(&mut jobs, |job| job.run(size, overlay, &index));
        } else {
            for job in &mut jobs {
                job.run(size, overlay, &index);
            }
        }
        for job in jobs {
            for watch in job.watches {
                self.vision.apply_watch(job.player, watch);
            }
            self.vision.players.insert(job.player, job.vision);
        }
    }
}
