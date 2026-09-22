//! Worker operations and bounded prepared map values.

use super::*;

/// A bounded native-worker operation passed on stdin by a direct process spawn.
#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WorkerRequest {
    PrepareOverviewElevation {
        cache_root: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
    },
    PrepareOverviewDirectory {
        cache_root: PathBuf,
        output_directory: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
    },
    PrepareDetailedDirectory {
        cache_root: PathBuf,
        output_directory: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
        resolution: DemResolution,
        #[serde(default)]
        staging_root: Option<PathBuf>,
    },
    ListOverviewSources,
    ListPotentialBiomeSources,
    ListHydeSources,
    InspectRaster {
        path: PathBuf,
    },
    ProjectPoint {
        center_latitude_e7: i32,
        center_longitude_e7: i32,
        longitude: f64,
        latitude: f64,
    },
    ProjectFootprint {
        request: MapRequest,
        samples_per_edge: u8,
    },
    PrepareElevation {
        path: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
    },
}

/// Source-backed overview data returned by the worker in a bounded response.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WorkerResponse {
    KnownSources {
        sources: Vec<KnownSource>,
    },
    RasterDimensions {
        width: usize,
        height: usize,
    },
    ProjectedPoint {
        east_meters: i64,
        north_meters: i64,
    },
    GeographicFootprint {
        points: Vec<GeographicPoint>,
        distortion: ProjectionDistortion,
    },
    PreparedElevation {
        environment: PreparedEnvironment,
        pages: Vec<ElevationPage>,
    },
    PreparedOverview(Box<PreparedOverview>),
    #[serde(rename = "prepared_directory")]
    PreparedDirectory {
        package: MapPackage,
    },
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct PreparedOverview {
    pub source_lock: aoe_map::SourceLock,
    pub water_source_lock: aoe_map::SourceLock,
    pub vegetation_source_lock: aoe_map::SourceLock,
    pub vegetation_classes_source_lock: aoe_map::SourceLock,
    pub hyde_baseline_source_lock: aoe_map::SourceLock,
    pub hyde_supplementary_source_lock: aoe_map::SourceLock,
    pub hyde_readme_source_lock: aoe_map::SourceLock,
    pub projection: ProjectionMetadata,
    pub provenance: EnvironmentalProvenance,
    pub environment: PreparedEnvironment,
    pub pages: Vec<ElevationPage>,
    pub water_pages: Vec<WaterPage>,
    pub vegetation_pages: Vec<PotentialBiomePage>,
    pub historical_land_use_pages: Vec<HistoricalLandUsePage>,
}

/// A self-contained, source-backed map result suitable for offline package
/// verification or later persistence by a server adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeneratedMap {
    pub package: MapPackage,
    pub elevation_pages: Vec<ElevationPage>,
    pub water_pages: Vec<WaterPage>,
    pub vegetation_pages: Vec<PotentialBiomePage>,
    pub historical_land_use_pages: Vec<HistoricalLandUsePage>,
    pub hydrology_evidence_pages: Vec<HydrologyEvidencePage>,
    pub modern_land_cover_pages: Vec<ModernLandCoverPage>,
}

impl GeneratedMap {
    pub fn from_prepared(
        request: MapRequest,
        prepared: PreparedOverview,
    ) -> Result<Self, GeodataError> {
        let package = MapPackage::with_prepared_environment(
            MAP_SCHEMA_VERSION,
            request,
            vec![
                prepared.source_lock,
                prepared.water_source_lock,
                prepared.vegetation_source_lock,
                prepared.vegetation_classes_source_lock,
                prepared.hyde_baseline_source_lock,
                prepared.hyde_supplementary_source_lock,
                prepared.hyde_readme_source_lock,
            ],
            prepared.projection,
            prepared.provenance,
            prepared.environment,
        )?;
        Ok(Self {
            package,
            elevation_pages: prepared.pages,
            water_pages: prepared.water_pages,
            vegetation_pages: prepared.vegetation_pages,
            historical_land_use_pages: prepared.historical_land_use_pages,
            hydrology_evidence_pages: Vec::new(),
            modern_land_cover_pages: Vec::new(),
        })
    }

    /// Confirms both the canonical manifest and every frozen page needed for
    /// terrain generation without asking a provider for additional data.
    pub fn validate(&self) -> Result<(), GeodataError> {
        self.package.validate()?;
        let generator = self.package.generator().with_prepared_elevation(
            self.package.request.compression,
            &self.package.environment,
            self.elevation_pages.clone(),
        )?;
        let generator =
            generator.with_prepared_water(&self.package.environment, self.water_pages.clone())?;
        let generator = generator
            .with_prepared_biomes(&self.package.environment, self.vegetation_pages.clone())?;
        let _generator = generator.with_historical_land_use(
            &self.package.environment,
            self.historical_land_use_pages.clone(),
        )?;
        if let Some(index) = &self.package.environment.hydrology_evidence {
            index.validate_pages(
                &self.hydrology_evidence_pages,
                &self.modern_land_cover_pages,
            )?;
        } else if !self.hydrology_evidence_pages.is_empty()
            || !self.modern_land_cover_pages.is_empty()
        {
            return Err(GeodataError::Preparation(
                "typed evidence pages require a package evidence index",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_worker_request_accepts_legacy_without_staging_scope() {
        let request: WorkerRequest = serde_json::from_value(serde_json::json!({
            "operation": "prepare_detailed_directory",
            "cache_root": "/cache",
            "output_directory": "/maps",
            "request": MapRequest::default(),
            "samples_per_axis": 128,
            "resolution": "glo90"
        }))
        .expect("legacy detailed request");
        assert!(matches!(
            request,
            WorkerRequest::PrepareDetailedDirectory {
                staging_root: None,
                ..
            }
        ));
    }
}
