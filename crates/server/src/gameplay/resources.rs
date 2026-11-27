use super::GameplayService;
use aoe_core::Tick;
use aoe_map::{Depletion, EnvironmentPageProvider, MapPackage, ResourceOverlaySnapshot};
use aoe_simulation::{GameWorld, GameWorldError, PreparedResourceDepletion, StartSearchResult};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[path = "resources/feed.rs"]
mod feed;
#[path = "resources/herds.rs"]
mod herds;
#[path = "resources/store.rs"]
mod store;
pub(super) use feed::Journal;
pub(super) use feed::{ResourceUpdate, send_resources};

#[derive(Debug, thiserror::Error)]
pub enum ResourceLifecycleError {
    #[error("navigation preparation failed: {0}")]
    Navigation(String),
    #[error("resource storage error: {0}")]
    Io(#[from] std::io::Error),
    #[error("resource snapshot JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("resource snapshot exceeds its byte limit")]
    TooLarge,
    #[error("resource persistence requires a configured map directory")]
    Disabled,
    #[error("resource mutation requires an immutable map world")]
    NoMap,
    #[error("stored resource checkpoint changed; reactivate the map before retrying")]
    StaleRevision,
    #[error("invalid resource state: {0}")]
    Overlay(#[from] aoe_map::ResourceOverlayError),
    #[error("invalid map world: {0}")]
    World(#[from] GameWorldError),
    #[error("resource persistence task failed")]
    Task,
}

pub struct PersistedDepletion {
    pub depletion: Depletion,
    /// False means replacement and live commit succeeded, but a crash could
    /// lose the directory update. It never means the mutation was rejected.
    pub directory_synced: bool,
}

pub(super) struct ResourcePersistence {
    directory: PathBuf,
    hash: [u8; 32],
    clock_origin: u64,
    // Accessed only while holding the exclusive world guard.
    checkpoint_required: AtomicBool,
    herds: herds::HerdPersistence,
}

impl ResourcePersistence {
    fn save_mutation(
        &self,
        transaction: &PreparedResourceDepletion<'_>,
    ) -> Result<bool, ResourceLifecycleError> {
        let synced = store::save(
            &self.directory,
            &transaction.snapshot(self.hash),
            transaction.previous_revision(),
            self.clock_origin.saturating_add(transaction.tick().0),
        )?;
        self.checkpoint_required.store(!synced, Ordering::Relaxed);
        Ok(synced)
    }

    pub(super) fn accept(&self, transaction: &PreparedResourceDepletion<'_>) -> bool {
        persistence_succeeded(self.save_mutation(transaction))
    }

    pub(super) fn checkpoint_recovery(&self, world: &GameWorld, before: Option<Tick>) {
        self.herds.step(world);
        let now = world.tick().0;
        let needs_snapshot = self.checkpoint_required.load(Ordering::Relaxed)
            || before.is_some_and(|deadline| deadline.0 <= now);
        if !needs_snapshot && (before.is_none() || world.resource_recovery_deadline().is_none()) {
            // A transaction that starts (or ends) recovery saved the schedule.
            return;
        }
        let tick = self.clock_origin.saturating_add(now);
        let Some(revision) = world.resource_revision() else {
            persistence_succeeded(Err(ResourceLifecycleError::NoMap));
            return;
        };
        let result = if needs_snapshot {
            // Due work can cancel or defer deadlines without changing amounts.
            let result = world
                .resource_snapshot(self.hash)
                .ok_or(ResourceLifecycleError::NoMap)
                .and_then(|snapshot| store::save(&self.directory, &snapshot, revision, tick));
            self.checkpoint_required
                .store(!matches!(result, Ok(true)), Ordering::Relaxed);
            result
        } else {
            store::save_clock(&self.directory, self.hash, revision, tick)
        };
        persistence_succeeded(result);
    }
}

fn persistence_succeeded(result: Result<bool, ResourceLifecycleError>) -> bool {
    match result {
        Ok(synced) => {
            if !synced {
                tracing::warn!("resource checkpoint directory sync failed");
            }
            true
        }
        Err(error) => {
            tracing::warn!("resource persistence refused: {error}");
            false
        }
    }
}

impl GameplayService {
    pub(crate) fn from_stored_map(
        package: MapPackage,
        provider: Option<Arc<dyn EnvironmentPageProvider>>,
        package_directory: Option<PathBuf>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Self>, ResourceLifecycleError> {
        let content_hash = package.content_hash;
        let metadata = crate::gameplay_map::map_metadata(&package);
        let mut world = if let Some(provider) = provider {
            GameWorld::from_page_provider(package, provider)?
        } else {
            if package.environment.samples_per_axis != 0 {
                return Err(GameWorldError::InvalidTerrain.into());
            }
            GameWorld::from_map(package)?
        };
        crate::map_store::navigation::prepare_world(
            &world,
            content_hash,
            package_directory.as_deref(),
            cancelled,
        )
        .map_err(ResourceLifecycleError::Navigation)?;
        world.set_tick_hz(crate::Config::sim_hz_from_env());
        let directory = package_directory.map(|root| root.join("resource-overlays"));
        let mut clock_origin = 0;
        let mut checkpoint_required = false;
        if let Some(directory) = &directory
            && let Some(checkpoint) = store::load(directory, content_hash)?
        {
            clock_origin = checkpoint.tick;
            checkpoint_required = !checkpoint.snapshot.recovery.is_empty()
                && checkpoint.snapshot.recovery_hz != world.config().tick_hz;
            world.restore_resources(&checkpoint.snapshot, content_hash, cancelled)?;
        }
        let config = world.config();
        let start = world
            .terrain()
            .search_start_checked(config, 64, cancelled)
            .map_err(GameWorldError::Environment)?;
        let tile = match start {
            StartSearchResult::Found(tile) => tile,
            StartSearchResult::Unavailable => return Ok(None),
            StartSearchResult::LimitReached | StartSearchResult::Cancelled => {
                return Err(GameWorldError::StartSearchLimit.into());
            }
        };
        let unit = crate::gameplay_map::initialize_village(&mut world, tile)?;
        let persistence = directory
            .map(|directory| {
                let herds = herds::HerdPersistence::open(&directory, content_hash, &mut world)?;
                Ok::<_, ResourceLifecycleError>(Arc::new(ResourcePersistence {
                    directory,
                    hash: content_hash,
                    clock_origin,
                    checkpoint_required: AtomicBool::new(checkpoint_required),
                    herds,
                }))
            })
            .transpose()?;
        let mut service = Self::from_world(world, unit, Some(content_hash), Some(metadata));
        service.resource_persistence = persistence;
        Ok(Some(service))
    }

    pub async fn resource_snapshot(&self) -> Option<ResourceOverlaySnapshot> {
        let hash = self.map_content_hash?;
        self.world.read().await.resource_snapshot(hash)
    }

    /// Persist before publishing collision changes. The exclusive world guard
    /// prevents ticks or another mutation between validation and commit.
    pub async fn deplete_resource_persisted(
        &self,
        id: u64,
        amount: u16,
    ) -> Result<PersistedDepletion, ResourceLifecycleError> {
        self.map_content_hash.ok_or(ResourceLifecycleError::NoMap)?;
        let persistence = self
            .resource_persistence
            .clone()
            .ok_or(ResourceLifecycleError::Disabled)?;
        let mut world = self.world.clone().write_owned().await;
        let journal = self.resource_journal.clone();
        tokio::task::spawn_blocking(move || {
            let mutation = world.prepare_resource_depletion(id, amount)?;
            let directory_synced = persistence.save_mutation(&mutation)?;
            let depletion = mutation.commit();
            journal
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .record(depletion);
            Ok(PersistedDepletion {
                depletion,
                directory_synced,
            })
        })
        .await
        .map_err(|_| ResourceLifecycleError::Task)?
    }
}

#[path = "resources/tests.rs"]
#[cfg(test)]
mod tests;
