use super::GameplayService;
use aoe_core::{PlayerId, WorldPosition};
use aoe_map::{Depletion, EnvironmentPageProvider, MapPackage, ResourceOverlaySnapshot};
use aoe_simulation::{GameWorld, GameWorldError, StartSearchResult};
use std::{path::PathBuf, sync::Arc};
#[path = "resources/feed.rs"]
mod feed;
#[path = "resources/store.rs"]
mod store;
pub(super) use feed::Journal;

#[derive(Debug, thiserror::Error)]
pub enum ResourceLifecycleError {
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
    #[error("stored resource revision changed; reactivate the map before retrying")]
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
        let directory = package_directory.map(|root| root.join("resource-overlays"));
        if let Some(directory) = &directory
            && let Some(snapshot) = store::load(directory, content_hash)?
        {
            world.restore_resources(&snapshot, content_hash, cancelled)?;
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
        let position =
            WorldPosition::from_tile_center(tile).map_err(|_| GameWorldError::InvalidPosition)?;
        let unit = world.spawn_unit(PlayerId(0), position)?;
        let mut service = Self::from_world(world, unit, Some(content_hash), Some(metadata));
        service.resource_directory = directory;
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
        let hash = self.map_content_hash.ok_or(ResourceLifecycleError::NoMap)?;
        let directory = self
            .resource_directory
            .clone()
            .ok_or(ResourceLifecycleError::Disabled)?;
        let mut world = self.world.clone().write_owned().await;
        let journal = self.resource_journal.clone();
        tokio::task::spawn_blocking(move || {
            let mutation = world.prepare_resource_depletion(id, amount)?;
            let snapshot = mutation.snapshot(hash);
            let directory_synced =
                store::save(&directory, &snapshot, mutation.previous_revision())?;
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
