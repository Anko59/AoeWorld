use crate::GameplayService;
use aoe_core::{PlayerId, WorldPosition};
use aoe_map::{
    ElevationPage, EnvironmentPageProvider, GAME_TILE_METERS, HistoricalLandUsePage, MapPackage,
    PotentialBiomePage, PreparedEnvironment, WaterPage,
};
use aoe_protocol::MapMetadata;
use aoe_simulation::{GameWorld, GameWorldError, StartSearchResult};
use std::sync::Arc;

impl GameplayService {
    pub fn from_map(package: MapPackage) -> Result<Self, GameWorldError> {
        let content_hash = package.content_hash;
        let metadata = map_metadata(&package);
        let mut world = GameWorld::from_map(package)?;
        let config = world.config();
        let tile = match world
            .terrain()
            .search_start_checked(config, 64, || false)
            .map_err(|_| GameWorldError::InvalidTerrain)?
        {
            StartSearchResult::Found(tile) => tile,
            StartSearchResult::Unavailable => return Err(GameWorldError::InvalidPosition),
            StartSearchResult::LimitReached | StartSearchResult::Cancelled => {
                return Err(GameWorldError::StartSearchLimit);
            }
        };
        let position =
            WorldPosition::from_tile_center(tile).map_err(|_| GameWorldError::InvalidPosition)?;
        let primary_unit_id = world.spawn_unit(PlayerId(0), position)?;
        Ok(Self::from_world(
            world,
            primary_unit_id,
            Some(content_hash),
            Some(metadata),
        ))
    }

    pub fn from_prepared_map(
        package: MapPackage,
        elevation_pages: Vec<ElevationPage>,
        water_pages: Vec<WaterPage>,
        vegetation_pages: Vec<PotentialBiomePage>,
        land_use_pages: Vec<HistoricalLandUsePage>,
    ) -> Result<Option<Self>, GameWorldError> {
        if all_ocean(&package.environment, &water_pages) {
            return Ok(None);
        }
        let content_hash = package.content_hash;
        let metadata = map_metadata(&package);
        let mut world = GameWorld::from_prepared_map(
            package,
            elevation_pages,
            water_pages,
            vegetation_pages,
            land_use_pages,
        )?;
        let config = world.config();
        let tile = match world
            .terrain()
            .search_start_checked(config, 64, || false)
            .map_err(|_| GameWorldError::InvalidTerrain)?
        {
            StartSearchResult::Found(tile) => tile,
            StartSearchResult::Unavailable => return Ok(None),
            StartSearchResult::LimitReached | StartSearchResult::Cancelled => {
                return Err(GameWorldError::StartSearchLimit);
            }
        };
        let position =
            WorldPosition::from_tile_center(tile).map_err(|_| GameWorldError::InvalidPosition)?;
        let primary_unit_id = world.spawn_unit(PlayerId(0), position)?;
        Ok(Some(Self::from_world(
            world,
            primary_unit_id,
            Some(content_hash),
            Some(metadata),
        )))
    }

    pub fn from_prepared_provider(
        package: MapPackage,
        provider: Arc<dyn EnvironmentPageProvider>,
    ) -> Result<Option<Self>, GameWorldError> {
        Self::from_prepared_provider_with_cancel(package, provider, &|| false)
    }

    pub fn from_prepared_provider_with_cancel(
        package: MapPackage,
        provider: Arc<dyn EnvironmentPageProvider>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Self>, GameWorldError> {
        let content_hash = package.content_hash;
        let metadata = map_metadata(&package);
        let mut world = GameWorld::from_page_provider(package, provider)?;
        let config = world.config();
        let tile = match world
            .terrain()
            .search_start_checked(config, 64, cancelled)
            .map_err(|_| GameWorldError::InvalidTerrain)?
        {
            StartSearchResult::Found(tile) => tile,
            StartSearchResult::Unavailable => return Ok(None),
            StartSearchResult::LimitReached | StartSearchResult::Cancelled => {
                return Err(GameWorldError::StartSearchLimit);
            }
        };
        let position =
            WorldPosition::from_tile_center(tile).map_err(|_| GameWorldError::InvalidPosition)?;
        let primary_unit_id = world.spawn_unit(PlayerId(0), position)?;
        Ok(Some(Self::from_world(
            world,
            primary_unit_id,
            Some(content_hash),
            Some(metadata),
        )))
    }
}

/// A root coverage value of 100 can only result from every level-zero ocean
/// sample being 100. This lets all-ocean maps remain available for preview
/// without enumerating their virtual game tiles during activation.
fn all_ocean(environment: &PreparedEnvironment, pages: &[WaterPage]) -> bool {
    let Some(root_level) = environment
        .water
        .as_ref()
        .and_then(|field| field.levels.len().checked_sub(1))
    else {
        return false;
    };
    pages.iter().any(|page| {
        usize::from(page.level) == root_level
            && page.x == 0
            && page.y == 0
            && page.width == 1
            && page.height == 1
            && page.ocean_coverage_percent == [100]
    })
}

pub(crate) fn map_metadata(package: &MapPackage) -> MapMetadata {
    MapMetadata {
        tile_size_meters: GAME_TILE_METERS as u8,
        compression_numerator: package.request.compression.numerator,
        compression_denominator: package.request.compression.denominator,
        terrain_schema_version: package.schema_version,
    }
}

#[path = "gameplay_map/tests.rs"]
#[cfg(test)]
mod tests;
