use crate::{
    ElevationPage, EnvironmentPageProvider, HistoricalLandUsePage, MapChunkGenerator, MapPackage,
    MapPackageError, PotentialBiomePage, WaterPage,
};
use std::sync::Arc;

impl MapPackage {
    /// Creates pure terrain queries backed by complete, verified elevation
    /// pages supplied by the server storage adapter.
    pub fn generator_with_elevation(
        &self,
        pages: Vec<ElevationPage>,
    ) -> Result<MapChunkGenerator, MapPackageError> {
        if self.environment.samples_per_axis == 0 {
            return pages
                .is_empty()
                .then(|| self.generator())
                .ok_or(MapPackageError::InvalidEnvironment);
        }
        self.generator()
            .with_prepared_elevation(self.request.compression, &self.environment, pages)
            .map_err(|_| MapPackageError::InvalidEnvironment)
    }

    /// Creates terrain queries with complete elevation and water page sets
    /// supplied by the server storage adapter.
    pub fn generator_with_environment(
        &self,
        elevation_pages: Vec<ElevationPage>,
        water_pages: Vec<WaterPage>,
        biome_pages: Vec<PotentialBiomePage>,
        land_use_pages: Vec<HistoricalLandUsePage>,
    ) -> Result<MapChunkGenerator, MapPackageError> {
        if self.environment.hydrology_evidence.is_some() {
            return Err(MapPackageError::InvalidEnvironment);
        }
        self.generator_with_elevation(elevation_pages)?
            .with_prepared_water(&self.environment, water_pages)
            .and_then(|generator| generator.with_prepared_biomes(&self.environment, biome_pages))
            .and_then(|generator| {
                generator.with_historical_land_use(&self.environment, land_use_pages)
            })
            .map_err(|_| MapPackageError::InvalidEnvironment)
    }

    /// Creates a lazy terrain generator backed by one-page-at-a-time source
    /// access. The provider retains no complete page vector; callers receive
    /// fallible query methods from [`MapChunkGenerator`].
    pub fn generator_with_page_provider(
        &self,
        provider: Arc<dyn EnvironmentPageProvider>,
    ) -> Result<MapChunkGenerator, MapPackageError> {
        if self.environment.samples_per_axis == 0 {
            return Err(MapPackageError::InvalidEnvironment);
        }
        self.generator()
            .with_page_provider(self.request.compression, self.environment.clone(), provider)
            .map_err(|_| MapPackageError::InvalidEnvironment)
    }
}
