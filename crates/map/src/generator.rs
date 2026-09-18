use crate::{
    ElevationPage, HistoricalLandUsePage, MapChunkGenerator, MapPackage, MapPackageError,
    PotentialBiomePage, WaterPage,
};

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
        self.generator_with_elevation(elevation_pages)?
            .with_prepared_water(&self.environment, water_pages)
            .and_then(|generator| generator.with_prepared_biomes(&self.environment, biome_pages))
            .and_then(|generator| {
                generator.with_historical_land_use(&self.environment, land_use_pages)
            })
            .map_err(|_| MapPackageError::InvalidEnvironment)
    }
}
