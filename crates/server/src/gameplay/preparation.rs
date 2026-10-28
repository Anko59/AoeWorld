//! Bounded immutable terrain warming outside the authoritative tick.
use aoe_core::TileCoord;
use aoe_map::MapChunkGenerator;
use std::collections::{BTreeSet, HashMap};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Semaphore;

#[path = "preparation/queue.rs"]
mod queue;

type Key = (i32, i32);
pub(super) type Ready = Arc<AtomicBool>;
const MAX_PENDING: usize = 64;
const WORKERS: usize = 2;
const COMMAND_RESERVE: usize = 8;

#[derive(Clone)]
pub(super) struct Preparation {
    slots: Arc<Semaphore>,
    workers: Arc<Semaphore>,
    viewport_slots: Arc<Semaphore>,
    jobs: Arc<Mutex<queue::Jobs>>,
    terrain_cache: Arc<Mutex<crate::terrain_cache::TerrainCache>>,
    pending: Arc<Mutex<HashMap<Key, Weak<AtomicBool>>>>,
    cancelled: Arc<AtomicBool>,
}
impl Default for Preparation {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(MAX_PENDING)),
            workers: Arc::new(Semaphore::new(WORKERS)),
            viewport_slots: Arc::new(Semaphore::new(MAX_PENDING)),
            jobs: Arc::default(),
            terrain_cache: Arc::default(),
            pending: Arc::default(),
            cancelled: Arc::default(),
        }
    }
}
impl Preparation {
    fn launch(
        &self,
        key: Key,
        work: impl FnOnce(&AtomicBool) + Send + 'static,
    ) -> Result<Ready, ()> {
        let mut pending = self.pending.lock().map_err(|_| ())?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(());
        }
        if let Some(known) = pending.get(&key).and_then(Weak::upgrade) {
            return Ok(known);
        }
        let ready = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        let result = self.submit(false, move || {
            let started = std::time::Instant::now();
            work(&cancelled);
            if started.elapsed() > std::time::Duration::from_millis(200) {
                tracing::info!(
                    chunk_x = key.0,
                    chunk_y = key.1,
                    elapsed_us = started.elapsed().as_micros() as u64,
                    "terrain chunk prepared off tick"
                );
            }
        })?;
        pending.insert(key, Arc::downgrade(&ready));
        let (pending, complete) = (self.pending.clone(), ready.clone());
        tokio::spawn(async move {
            let _ = result.await;
            // Publish readiness before leaving `pending`, so that an empty
            // `pending` map implies every launched flag is already set.
            complete.store(true, Ordering::Release);
            if let Ok(mut pending) = pending.lock() {
                pending.remove(&key);
            }
        });
        Ok(ready)
    }

    /// True while a terrain warm-up is still running.
    fn busy(&self) -> bool {
        self.pending.lock().is_ok_and(|pending| !pending.is_empty())
    }

    pub(super) fn close(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.workers.close();
    }

    pub(super) fn chunks(generator: &MapChunkGenerator, tiles: &[TileCoord]) -> BTreeSet<Key> {
        let side = generator.width_tiles().div_euclid(aoe_map::CHUNK_TILES)
            + i32::from(generator.width_tiles().rem_euclid(aoe_map::CHUNK_TILES) != 0);
        tiles
            .iter()
            .flat_map(|tile| {
                let (x, y) = (
                    tile.x.div_euclid(aoe_map::CHUNK_TILES),
                    tile.y.div_euclid(aoe_map::CHUNK_TILES),
                );
                (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
            })
            .filter(|(x, y)| *x >= 0 && *y >= 0 && *x < side && *y < side)
            .collect()
    }

    /// At most two new chunks per tick; the shared admission cap bounds all
    /// background hints and command warmups together.
    pub(super) fn prefetch(&self, generator: MapChunkGenerator, tiles: &[TileCoord]) {
        if generator.has_static_navigation() {
            return;
        }
        let mut launched = 0;
        for (x, y) in Self::chunks(&generator, tiles) {
            if generator.walk_chunk_resident(x, y) {
                continue;
            }
            if self
                .pending
                .lock()
                .is_ok_and(|pending| pending.contains_key(&(x, y)))
            {
                continue;
            }
            if launched == WORKERS || self.slots.available_permits() <= COMMAND_RESERVE {
                break;
            }
            let generator = generator.clone();
            if self
                .launch((x, y), move |cancelled| {
                    let _ = generator
                        .precompose_walk_chunk(x, y, &|| cancelled.load(Ordering::Acquire));
                })
                .is_err()
            {
                break;
            }
            launched += 1;
        }
    }

    pub(super) fn prepare(
        &self,
        generator: MapChunkGenerator,
        tiles: &[TileCoord],
    ) -> Result<Vec<Ready>, ()> {
        if generator.has_static_navigation() {
            return Ok(Vec::new());
        }
        let mut ready = Vec::new();
        // Cold command admission warms only its actual endpoints. Speculative
        // neighbour halos are submitted separately under the per-tick cap.
        let keys = tiles
            .iter()
            .map(|tile| {
                (
                    tile.x.div_euclid(aoe_map::CHUNK_TILES),
                    tile.y.div_euclid(aoe_map::CHUNK_TILES),
                )
            })
            .collect::<BTreeSet<_>>();
        for (x, y) in keys {
            if generator.walk_chunk_resident(x, y) {
                continue;
            }
            let generator = generator.clone();
            ready.push(self.launch((x, y), move |cancelled| {
                if let Err(error) =
                    generator.precompose_walk_chunk(x, y, &|| cancelled.load(Ordering::Acquire))
                {
                    tracing::warn!(x, y, ?error, "terrain preparation refused");
                }
            })?);
        }
        Ok(ready)
    }
}

#[path = "preparation/tests.rs"]
#[cfg(test)]
mod tests;

impl super::GameplayService {
    /// Returns once every terrain warm-up launched so far has finished, so
    /// each command accepted before the call is applied by the next tick.
    ///
    /// Commands wait for their terrain, which is warmed on background
    /// threads, so the tick that applies a command otherwise depends on
    /// wall-clock timing. A harness that needs one tick-exact history awaits
    /// this after issuing commands; a live match never needs it.
    pub async fn settle_preparation(&self) {
        while self.preparation.busy() {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }

    pub(crate) fn install_initial_terrain(&self, cache: crate::terrain_cache::TerrainCache) {
        if let Ok(mut target) = self.preparation.terrain_cache.lock() {
            *target = cache;
        }
    }

    pub(crate) fn initial_terrain_chunk(
        &self,
        key: &crate::terrain_cache::Key,
    ) -> Option<aoe_map::CompactChunk> {
        self.preparation.terrain_cache.lock().ok()?.get(key)
    }

    pub(crate) fn cancel_preparation(&self) {
        self.preparation.close();
    }

    pub(super) async fn prefetch_routes(&self) {
        let request = {
            let world = self.world.read().await;
            match world.terrain() {
                aoe_simulation::Terrain::Map { generator, .. } => {
                    Some((generator.clone(), world.navigation_preparation_tiles(32)))
                }
                _ => None,
            }
        };
        if let Some((generator, tiles)) = request {
            self.preparation.prefetch(generator, &tiles);
        }
    }
}

pub(super) struct Request {
    pub(super) owner: aoe_core::PlayerId,
    pub(super) generator: MapChunkGenerator,
    pub(super) tiles: [TileCoord; 2],
}
impl Request {
    pub(super) fn new(
        world: &aoe_simulation::GameWorld,
        action: &super::PendingAction,
    ) -> Option<Self> {
        let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() else {
            return None;
        };
        let (unit, origin, destination) = match action {
            super::PendingAction::Move {
                entity_id,
                destination,
                ..
            } => {
                let unit = world.unit(*entity_id).filter(|u| u.hp > 0)?;
                (unit, unit.position, *destination)
            }
            super::PendingAction::Formation {
                unit_ids,
                destination,
                ..
            } => {
                if unit_ids.is_empty() || unit_ids.len() > aoe_core::MAX_FORMATION_MEMBERS {
                    return None;
                }
                let unit = world.unit(unit_ids[0]).filter(|u| u.hp > 0)?;
                let (x, y) = unit_ids.iter().try_fold((0_i64, 0_i64), |(x, y), id| {
                    world
                        .unit(*id)
                        .map(|u| (x + i64::from(u.position.x), y + i64::from(u.position.y)))
                })?;
                (
                    unit,
                    aoe_core::WorldPosition::new(
                        (x / unit_ids.len() as i64) as i32,
                        (y / unit_ids.len() as i64) as i32,
                    ),
                    destination.unwrap_or(aoe_core::WorldPosition::new(
                        (x / unit_ids.len() as i64) as i32,
                        (y / unit_ids.len() as i64) as i32,
                    )),
                )
            }
            _ => return None,
        };
        Some(Self {
            owner: unit.player,
            generator: generator.clone(),
            tiles: [
                origin.tile_floor(),
                world
                    .config()
                    .snap_ground_position(destination)
                    .tile_floor(),
            ],
        })
    }
}
