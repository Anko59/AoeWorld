use super::{Result, Seed, write};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, HYDROLOGY_WATER_MODEL_VERSION, HydrologyEvidenceIndex,
    HydrologyEvidencePage, HydrologyKind, HydrologyWaterModelIndex, HydrologyWaterModelPage,
    HydrologyWaterPolicy, MAP_SCHEMA_VERSION, MapPackage, MapRequest, ModernLandCoverPage,
    PreparedEnvironment, PyramidLevel, WATER_CORRECTION_TARGET_YEAR_CE,
    WORLD_COVER_OBSERVATION_YEAR, WaterCorrectionDocument, WaterFlowDirection,
    WaterModelProvenance, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
};
use std::path::Path;

pub(super) fn prepare(
    root: &Path,
    request: MapRequest,
    evidence: &HydrologyEvidencePage,
    land_cover: &ModernLandCoverPage,
) -> Result<Vec<Seed>> {
    let samples = evidence.kind.len();
    let mut modeled_page = evidence.clone();
    modeled_page.water_model = Some(HydrologyWaterModelPage {
        kind: evidence.kind.clone(),
        surface_level_centimeters: evidence
            .kind
            .iter()
            .map(|kind| (*kind == HydrologyKind::Lake as u8).then_some(950))
            .collect(),
        flow_direction: vec![WaterFlowDirection::Unknown as u8; samples],
        provenance: evidence
            .kind
            .iter()
            .map(|kind| {
                if *kind == HydrologyKind::Lake as u8 {
                    WaterModelProvenance::ModelledLakeSurface as u8
                } else {
                    WaterModelProvenance::EvidenceOnly as u8
                }
            })
            .collect(),
    });
    modeled_page.validate()?;

    let mut seeds = Vec::new();
    for (version, name) in [(1, "v1"), (HYDROLOGY_WATER_MODEL_VERSION, "v2")] {
        let page_bytes = serde_json::to_vec(&modeled_page)?;
        seeds.push(write(
            root,
            "environment_page",
            &format!("modeled-water-page-{name}"),
            &page_bytes,
        )?);

        let model = HydrologyWaterModelIndex {
            model_version: version,
            samples_per_axis: u16::from(evidence.width),
            target_year_ce: WATER_CORRECTION_TARGET_YEAR_CE,
            correction_document: WaterCorrectionDocument::empty(
                request,
                u16::from(evidence.width),
            )?,
        };
        model.validate()?;
        let index = HydrologyEvidenceIndex {
            samples_per_axis: u16::from(evidence.width),
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&modeled_page))?,
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(
                std::slice::from_ref(land_cover),
            )?,
            water_model: Some(model),
        };
        let environment = PreparedEnvironment {
            samples_per_axis: u16::from(evidence.width),
            geographic_millimeters_per_sample: 1_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: vec![
                    PyramidLevel {
                        samples_per_axis: u16::from(evidence.width),
                        ordered_page_root: [1; 32],
                    },
                    PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: [2; 32],
                    },
                ],
            },
            hydrology_evidence: Some(index),
            ..PreparedEnvironment::default()
        };
        let package = MapPackage::with_prepared_environment(
            MAP_SCHEMA_VERSION,
            request,
            Vec::new(),
            Default::default(),
            Default::default(),
            environment,
        )?;
        package.validate()?;
        seeds.push(write(
            root,
            "map_package",
            &format!("modeled-water-{name}"),
            &serde_json::to_vec(&package)?,
        )?);
    }
    Ok(seeds)
}
