//! Map-only wildlife journal. Stock changes checkpoint independently of terrain
//! harvests. Between stock changes only the pending birth clock is written.
use super::{ResourceLifecycleError, persistence_succeeded, store};
use aoe_simulation::{GameWorld, MapHerdSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Deserialize, Serialize)]
struct Checkpoint {
    map_content_hash: [u8; 32],
    generation: u64,
    tick: u64,
    stock: MapHerdSnapshot,
}

#[derive(Default)]
struct Written {
    generation: u64,
    live_revision: u64,
    retry: bool,
}

pub(super) struct HerdPersistence {
    directory: PathBuf,
    hash: [u8; 32],
    byte_limit: u64,
    clock_origin: u64,
    written: Mutex<Written>,
}

impl HerdPersistence {
    pub(super) fn open(
        overlays: &Path,
        hash: [u8; 32],
        world: &mut GameWorld,
    ) -> Result<Self, ResourceLifecycleError> {
        let directory = overlays.join("herds");
        // Worst-case JSON for each animal and its membership, plus metadata.
        let byte_limit = world.limits().animals as u64 * 512 + 1024;
        let checkpoint = load(&directory, hash, byte_limit)?;
        let mut written = Written::default();
        let mut clock_origin = 0;
        if let Some(checkpoint) = checkpoint {
            clock_origin = checkpoint.tick;
            written.generation = checkpoint.generation;
            written.retry = checkpoint.stock.has_pending_births()
                && checkpoint.stock.tick_hz() != world.config().tick_hz;
            world.restore_map_herds(checkpoint.stock)?;
        }
        written.live_revision = world.herd_recovery_revision();
        Ok(Self {
            directory,
            hash,
            byte_limit,
            clock_origin,
            written: Mutex::new(written),
        })
    }

    pub(super) fn step(&self, world: &GameWorld) {
        persistence_succeeded(self.persist(world));
    }

    fn persist(&self, world: &GameWorld) -> Result<bool, ResourceLifecycleError> {
        let mut written = self
            .written
            .lock()
            .map_err(|_| ResourceLifecycleError::Task)?;
        let revision = world.herd_recovery_revision();
        let tick = self.clock_origin.saturating_add(world.tick().0);
        if written.retry || revision != written.live_revision {
            let stock = world.map_herd_snapshot();
            let generation = written
                .generation
                .checked_add(1)
                .ok_or(ResourceLifecycleError::StaleRevision)?;
            let checkpoint = Checkpoint {
                map_content_hash: self.hash,
                generation,
                tick,
                stock,
            };
            let synced = save(&self.directory, self.hash, self.byte_limit, &checkpoint)?;
            *written = Written {
                generation,
                live_revision: revision,
                retry: !synced,
            };
            Ok(synced)
        } else if world.herd_recovery_pending() {
            store::save_clock(&self.directory, self.hash, written.generation, tick)
        } else {
            Ok(true)
        }
    }
}

fn load(
    directory: &Path,
    hash: [u8; 32],
    byte_limit: u64,
) -> Result<Option<Checkpoint>, ResourceLifecycleError> {
    let _guard = store::SAVE_LOCK
        .lock()
        .map_err(|_| ResourceLifecycleError::Task)?;
    let Some(mut checkpoint) = read(directory, hash, byte_limit)? else {
        return Ok(None);
    };
    if checkpoint.generation == 0 {
        return Err(aoe_map::ResourceOverlayError::InvalidSnapshot.into());
    }
    if checkpoint.stock.has_pending_births()
        && let Some((generation, tick)) = store::load_clock(directory, hash)?
        && generation == checkpoint.generation
        && tick > checkpoint.tick
    {
        checkpoint.stock.elapse(tick - checkpoint.tick)?;
        checkpoint.tick = tick;
    }
    Ok(Some(checkpoint))
}

fn read(
    directory: &Path,
    hash: [u8; 32],
    byte_limit: u64,
) -> Result<Option<Checkpoint>, ResourceLifecycleError> {
    let checkpoint: Option<Checkpoint> =
        store::load_json(&store::path(directory, hash), byte_limit)?;
    if checkpoint
        .as_ref()
        .is_some_and(|value| value.map_content_hash != hash)
    {
        return Err(aoe_map::ResourceOverlayError::WrongMap.into());
    }
    Ok(checkpoint)
}

fn save(
    directory: &Path,
    hash: [u8; 32],
    byte_limit: u64,
    checkpoint: &Checkpoint,
) -> Result<bool, ResourceLifecycleError> {
    let _guard = store::SAVE_LOCK
        .lock()
        .map_err(|_| ResourceLifecycleError::Task)?;
    let current = read(directory, hash, byte_limit)?;
    let previous = current.as_ref().map_or(0, |state| state.generation);
    if previous.checked_add(1) != Some(checkpoint.generation)
        || current.is_some_and(|state| state.tick > checkpoint.tick)
        || store::load_clock(directory, hash)?
            .is_some_and(|clock| clock > (previous, checkpoint.tick))
    {
        return Err(ResourceLifecycleError::StaleRevision);
    }
    let bytes = serde_json::to_vec(checkpoint)?;
    if bytes.len() as u64 > byte_limit {
        return Err(ResourceLifecycleError::TooLarge);
    }
    store::replace(directory, &store::path(directory, hash), &bytes)
}
