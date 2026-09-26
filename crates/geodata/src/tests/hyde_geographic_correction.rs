use super::*;

const PREPROCESSING: &str = "hyde-600ad-area-pages-v2";

fn request() -> MapRequest {
    MapRequest::default()
}

fn source(id: &str) -> HistoricalSourceCitation {
    HistoricalSourceCitation {
        id: id.to_owned(),
        citation: format!("Fixture citation for {id}"),
    }
}

fn quantities(valid: f64, crop: f64) -> HistoricalQuantityPatch {
    HistoricalQuantityPatch {
        valid_land_area_square_meters: valid,
        crop_area_square_kilometers: crop,
        grazing_area_square_kilometers: 0.0,
        population: 2.0,
    }
}

#[test]
fn empty_document_has_stable_digest_and_rejects_other_geography_grid_and_year() {
    let document = GeographicHistoricalCorrectionDocument::empty(request(), 2, PREPROCESSING)
        .expect("empty correction set");
    let digest = document.canonical_digest(request()).unwrap();
    let bytes = document.serialize(request()).unwrap();
    let decoded =
        GeographicHistoricalCorrectionDocument::deserialize(&bytes, request(), 2, PREPROCESSING)
            .unwrap();
    assert_eq!(decoded.canonical_digest(request()).unwrap(), digest);
    let mut different_location = request();
    different_location.center_longitude_e7 += 1_000_000;
    assert!(
        document
            .validate_for(different_location, 2, PREPROCESSING)
            .is_err()
    );
    assert!(document.validate_for(request(), 3, PREPROCESSING).is_err());
    assert!(
        document
            .validate_for(request(), 2, "other-preprocessing")
            .is_err()
    );
    let mut wrong_year = document;
    wrong_year.target_year_ce = 1900;
    assert!(
        wrong_year
            .validate_for(request(), 2, PREPROCESSING)
            .is_err()
    );
}

#[test]
fn only_cited_historical_evidence_changes_quantities_and_unknown_clears_valid_land() {
    let mut document =
        GeographicHistoricalCorrectionDocument::empty(request(), 2, PREPROCESSING).unwrap();
    document.sources = vec![source("historical"), source("modern"), source("unknown")];
    document.corrections = vec![
        GeographicHistoricalCorrection {
            cell_x: 0,
            cell_y: 0,
            source_id: "historical".into(),
            evidence: GeographicHistoricalEvidence::HistoricalModel {
                quantities: quantities(500_000.0, 0.25),
            },
        },
        GeographicHistoricalCorrection {
            cell_x: 1,
            cell_y: 0,
            source_id: "modern".into(),
            evidence: GeographicHistoricalEvidence::ModernObservation {
                observation_year_ce: 2026,
                quantities: quantities(500_000.0, 0.25),
            },
        },
        GeographicHistoricalCorrection {
            cell_x: 0,
            cell_y: 1,
            source_id: "unknown".into(),
            evidence: GeographicHistoricalEvidence::Unknown,
        },
    ];
    document.validate_for(request(), 2, PREPROCESSING).unwrap();
    assert!(document.changes_historical_land_use());
    let original = HydeAreaAllocation {
        land_area_square_meters: 1_000_000.0,
        valid_land_area_square_meters: 1_000_000.0,
        crop_area_square_kilometers: 0.1,
        population: 1.0,
        ..HydeAreaAllocation::default()
    };
    let mut cells = vec![original; 4];
    document
        .apply_page(request(), 0, 0, 2, 2, &mut cells)
        .unwrap();
    assert_eq!(cells[0].valid_land_area_square_meters, 500_000.0);
    assert_eq!(cells[0].crop_area_square_kilometers, 0.25);
    assert_eq!(cells[1], original);
    assert_eq!(cells[2].land_area_square_meters, 1_000_000.0);
    assert_eq!(cells[2].valid_land_area_square_meters, 0.0);
    assert_eq!(cells[2].crop_area_square_kilometers, 0.0);
    assert_eq!(cells[3], original);
    document.corrections[0].source_id = "missing".into();
    assert!(document.validate_for(request(), 2, PREPROCESSING).is_err());
}

#[test]
fn correction_cannot_claim_more_valid_land_than_observed() {
    let mut document =
        GeographicHistoricalCorrectionDocument::empty(request(), 2, PREPROCESSING).unwrap();
    document.sources = vec![source("historical")];
    document.corrections = vec![GeographicHistoricalCorrection {
        cell_x: 0,
        cell_y: 0,
        source_id: "historical".into(),
        evidence: GeographicHistoricalEvidence::HistoricalModel {
            quantities: quantities(2_000_000.0, 0.25),
        },
    }];
    let mut cells = vec![
        HydeAreaAllocation {
            land_area_square_meters: 1_000_000.0,
            valid_land_area_square_meters: 1_000_000.0,
            ..HydeAreaAllocation::default()
        };
        4
    ];
    assert!(
        document
            .apply_page(request(), 0, 0, 2, 2, &mut cells)
            .is_err()
    );
}

#[test]
fn modern_observation_alone_does_not_claim_historical_correction_provenance() {
    let mut document =
        GeographicHistoricalCorrectionDocument::empty(request(), 2, PREPROCESSING).unwrap();
    document.sources = vec![source("modern")];
    document.corrections = vec![GeographicHistoricalCorrection {
        cell_x: 0,
        cell_y: 0,
        source_id: "modern".into(),
        evidence: GeographicHistoricalEvidence::ModernObservation {
            observation_year_ce: 2026,
            quantities: quantities(500_000.0, 0.25),
        },
    }];
    document.validate_for(request(), 2, PREPROCESSING).unwrap();
    assert!(!document.changes_historical_land_use());
}

#[test]
fn inline_document_stays_below_worker_request_budget() {
    let mut document =
        GeographicHistoricalCorrectionDocument::empty(request(), 2, PREPROCESSING).unwrap();
    document.sources = (0..60)
        .map(|index| HistoricalSourceCitation {
            id: format!("source-{index:03}"),
            citation: "x".repeat(500),
        })
        .collect();
    assert!(document.validate_for(request(), 2, PREPROCESSING).is_err());
}
