use super::*;

fn request() -> MapRequest {
    MapRequest::default()
}

fn patch(id: &str, priority: u16, operation: VegetationPatchOperation) -> VegetationPatch {
    VegetationPatch {
        id: id.into(),
        source_id: "historical-model".into(),
        priority,
        rectangle_east_north_meters: [-10_000, -10_000, 10_000, 10_000],
        applicable_year_start_ce: 500,
        applicable_year_end_ce: 700,
        operation,
    }
}

#[test]
fn bound_document_has_stable_digest_and_rejects_other_footprints() {
    let doc = VegetationPatchDocument::empty(request(), 2).unwrap();
    doc.validate_for(request(), 2).unwrap();
    let digest = doc.digest_hex(request()).unwrap();
    assert_eq!(digest.len(), 64);
    assert_eq!(doc.digest_hex(request()).unwrap(), digest);
    let mut elsewhere = request();
    elsewhere.center_longitude_e7 += 1_000_000;
    assert!(doc.validate_for(elsewhere, 2).is_err());
    assert!(doc.validate_for(request(), 3).is_err());
    let mut wrong_year = doc;
    wrong_year.target_year_ce = 2026;
    assert!(wrong_year.validate_for(request(), 2).is_err());
}

#[test]
fn cited_patch_precedence_applies_only_historical_operations() {
    let mut doc = VegetationPatchDocument::empty(request(), 2).unwrap();
    doc.sources.push(VegetationPatchSource {
        id: "historical-model".into(),
        citation: "Fixture reconstruction".into(),
    });
    doc.patches = vec![
        patch(
            "a",
            0,
            VegetationPatchOperation::HistoricalBiome { class: 27 },
        ),
        patch(
            "b",
            1,
            VegetationPatchOperation::ModernObservation {
                observation_year_ce: 2026,
                class: 15,
            },
        ),
        patch("c", 2, VegetationPatchOperation::Unknown),
    ];
    doc.validate_for(request(), 2).unwrap();
    let mut classes = vec![16; 4];
    doc.apply(2, &mut classes).unwrap();
    assert_eq!(classes, [0, 0, 0, 0]);
    doc.patches.pop();
    doc.apply(2, &mut classes).unwrap();
    assert_eq!(classes, [27, 27, 27, 27]);
    doc.patches[0].source_id = "uncited".into();
    assert!(doc.validate_for(request(), 2).is_err());
}

#[test]
fn patch_year_geometry_and_class_are_validated() {
    let mut doc = VegetationPatchDocument::empty(request(), 2).unwrap();
    doc.sources.push(VegetationPatchSource {
        id: "historical-model".into(),
        citation: "Fixture reconstruction".into(),
    });
    doc.patches.push(patch(
        "a",
        0,
        VegetationPatchOperation::HistoricalBiome { class: 27 },
    ));
    doc.validate_for(request(), 2).unwrap();
    doc.patches[0].applicable_year_start_ce = 601;
    assert!(doc.validate_for(request(), 2).is_err());
    doc.patches[0].applicable_year_start_ce = 500;
    doc.patches[0].rectangle_east_north_meters = [1, -1, 0, 1];
    assert!(doc.validate_for(request(), 2).is_err());
    doc.patches[0].rectangle_east_north_meters = [-10_000, -10_000, 10_000, 10_000];
    doc.patches[0].operation = VegetationPatchOperation::HistoricalBiome { class: 255 };
    assert!(doc.validate_for(request(), 2).is_err());
}
