use super::*;

#[test]
fn evidence_precedence_keeps_modern_water_observations_typed() {
    assert_eq!(
        classify_evidence(true, Some(HydrologyKind::Lake), true, 40),
        (HydrologyKind::Ocean, HydrologyEvidenceMethod::OverviewOcean)
    );
    assert_eq!(
        classify_evidence(false, Some(HydrologyKind::RegulatedLake), true, 80),
        (
            HydrologyKind::RegulatedLake,
            HydrologyEvidenceMethod::HydroLakesExtent
        )
    );
    assert_eq!(
        classify_evidence(false, None, true, 40),
        (
            HydrologyKind::River,
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor
        )
    );
    for class in [WORLD_COVER_WETLAND, 95] {
        assert_eq!(
            classify_evidence(false, None, false, class),
            (
                HydrologyKind::Shallow,
                HydrologyEvidenceMethod::WorldCoverClass
            )
        );
    }
    assert_eq!(
        classify_evidence(false, None, false, WORLD_COVER_PERMANENT_WATER),
        (
            HydrologyKind::UnknownWater,
            HydrologyEvidenceMethod::WorldCoverClass
        )
    );
    assert_eq!(
        classify_evidence(false, None, false, WORLD_COVER_NODATA),
        (HydrologyKind::NoEvidence, HydrologyEvidenceMethod::None)
    );
    assert_eq!(
        classify_evidence(false, None, false, 40),
        (
            HydrologyKind::Land,
            HydrologyEvidenceMethod::WorldCoverClass
        )
    );
}
