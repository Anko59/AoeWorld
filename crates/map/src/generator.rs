use crate::{ElevationPage, MapChunkGenerator, MapPackage, MapPackageError, WaterPage};

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
    ) -> Result<MapChunkGenerator, MapPackageError> {
        self.generator_with_elevation(elevation_pages)?
            .with_prepared_water(&self.environment, water_pages)
            .map_err(|_| MapPackageError::InvalidEnvironment)
    }
}
