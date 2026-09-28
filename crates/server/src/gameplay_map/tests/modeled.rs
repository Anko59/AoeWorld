use super::*;
use aoe_map::{
    HYDROLOGY_WATER_MODEL_VERSION, HydrologyEvidenceIndex, HydrologyEvidenceMethod,
    HydrologyEvidencePage, HydrologyKind, HydrologyWaterModelIndex, HydrologyWaterModelPage,
    HydrologyWaterPolicy, ModernLandCoverPage, WATER_MODEL_GENERATION_RECIPE_VERSION,
    WORLD_COVER_OBSERVATION_YEAR, WaterCorrectionDocument, WaterFlowDirection,
    WaterModelProvenance, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
};

#[derive(Debug)]
struct ModeledProvider {
    elevation: ElevationProvider,
    hydrology: Arc<EnvironmentPage>,
    land_cover: Arc<EnvironmentPage>,
}

impl EnvironmentPageProvider for ModeledProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if (key.level, key.x, key.y) == (0, 0, 0) {
            match key.layer {
                PageLayer::HydrologyEvidence => return Ok(self.hydrology.clone()),
                PageLayer::ModernLandCover => return Ok(self.land_cover.clone()),
                _ => {}
            }
        }
        self.elevation.page(key, cancelled)
    }
}

#[test]
fn modeled_water_package_activates_and_cancellation_remains_bounded() {
    let (base, _) = flat_prepared_package();
    let hydrology = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::Land as u8; 4],
        method: vec![HydrologyEvidenceMethod::WorldCoverClass as u8; 4],
        water_model: Some(HydrologyWaterModelPage {
            kind: vec![HydrologyKind::Land as u8; 4],
            surface_level_centimeters: vec![None; 4],
            flow_direction: vec![WaterFlowDirection::Unknown as u8; 4],
            provenance: vec![WaterModelProvenance::EvidenceOnly as u8; 4],
        }),
    };
    let land_cover = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![30; 4],
    };
    let mut environment = base.environment.clone();
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 2,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&hydrology))
            .expect("valid hydrology"),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(std::slice::from_ref(
            &land_cover,
        ))
        .expect("valid land cover"),
        water_model: Some(HydrologyWaterModelIndex {
            model_version: HYDROLOGY_WATER_MODEL_VERSION,
            samples_per_axis: 2,
            target_year_ce: 600,
            correction_document: WaterCorrectionDocument::empty(base.request, 2)
                .expect("corrections"),
        }),
    });
    let package = MapPackage::with_prepared_environment(
        base.generator_version,
        base.request,
        base.source_locks,
        base.projection,
        base.provenance,
        environment,
    )
    .expect("modeled package");
    package.validate().expect("valid package identity");
    assert_eq!(
        package.generation_recipe_version,
        WATER_MODEL_GENERATION_RECIPE_VERSION
    );
    let provider = Arc::new(ModeledProvider {
        elevation: elevation_provider(),
        hydrology: Arc::new(EnvironmentPage::HydrologyEvidence(hydrology)),
        land_cover: Arc::new(EnvironmentPage::ModernLandCover(land_cover)),
    });
    assert!(matches!(
        GameplayService::from_prepared_provider_with_cancel(
            package.clone(),
            provider.clone(),
            &|| true,
        ),
        Err(GameWorldError::StartSearchLimit)
    ));
    assert!(
        GameplayService::from_prepared_provider(package, provider)
            .expect("modeled provider activation")
            .is_some()
    );
}
