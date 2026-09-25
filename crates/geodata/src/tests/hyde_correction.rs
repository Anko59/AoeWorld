use super::*;
use crate::HydeAreaAllocation;

fn quantities() -> HydeWholeCellQuantities {
    HydeWholeCellQuantities {
        land_area_square_meters: 100_000_000.0,
        crop_area_square_kilometers: 25.0,
        grazing_area_square_kilometers: 10.0,
        population: 1_250.0,
    }
}

fn record(
    cell_x: u16,
    cell_y: u16,
    evidence: HistoricalCorrectionEvidence,
) -> HistoricalCorrection {
    HistoricalCorrection {
        cell_x,
        cell_y,
        evidence,
    }
}

fn document(corrections: Vec<HistoricalCorrection>) -> HistoricalCorrectionDocument {
    HistoricalCorrectionDocument::new(2, corrections).expect("valid correction document")
}

fn document_for_axis(
    samples_per_axis: u16,
    corrections: Vec<HistoricalCorrection>,
) -> HistoricalCorrectionDocument {
    HistoricalCorrectionDocument::new(samples_per_axis, corrections)
        .expect("valid correction document")
}

fn assert_correction_error(result: Result<(), GeodataError>, expected: &str) {
    assert!(
        matches!(
            result,
            Err(GeodataError::HistoricalCorrection(reason)) if reason.contains(expected)
        ),
        "expected correction error containing {expected:?}"
    );
}

#[test]
fn versioned_unknown_evidence_has_an_exact_canonical_json_fixture() {
    let document = document(vec![record(1, 0, HistoricalCorrectionEvidence::Unknown)]);
    let bytes = document.serialize().expect("serialized unknown evidence");

    assert_eq!(
        String::from_utf8(bytes.clone()).expect("UTF-8 JSON"),
        r#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":2,"corrections":[{"cell_x":1,"cell_y":0,"evidence":{"kind":"unknown"}}]}"#
    );
    assert_eq!(
        HistoricalCorrectionDocument::deserialize(&bytes).expect("round trip"),
        document
    );
}

#[test]
fn all_evidence_variants_round_trip_in_canonical_row_major_order() {
    let unsorted = vec![
        record(
            0,
            1,
            HistoricalCorrectionEvidence::FallbackEvidence {
                quantities: quantities(),
            },
        ),
        record(
            1,
            0,
            HistoricalCorrectionEvidence::ModernObservation {
                observation_year_ce: 2_021,
                quantities: quantities(),
            },
        ),
        record(1, 1, HistoricalCorrectionEvidence::Unknown),
        record(
            0,
            0,
            HistoricalCorrectionEvidence::Circa600Hyde {
                quantities: quantities(),
            },
        ),
        record(
            2,
            0,
            HistoricalCorrectionEvidence::ProceduralAddition {
                quantities: quantities(),
            },
        ),
    ];
    let document = document_for_axis(3, unsorted);
    let bytes = document.serialize().expect("serialized document");
    let decoded = HistoricalCorrectionDocument::deserialize(&bytes).expect("decoded document");

    assert_eq!(decoded, document);
    assert_eq!(decoded.serialize().expect("reencoded document"), bytes);
    assert_eq!(decoded.historical_corrections().count(), 1);
    for record in &decoded.corrections {
        match &record.evidence {
            HistoricalCorrectionEvidence::ProceduralAddition { .. }
            | HistoricalCorrectionEvidence::FallbackEvidence { .. } => {
                assert!(record.historical_quantities().is_none());
                assert_eq!(record.evidence_year_ce(), None);
            }
            _ => {}
        }
    }
}

#[test]
fn modern_evidence_round_trips_but_never_becomes_historical_quantities() {
    let modern = record(
        0,
        0,
        HistoricalCorrectionEvidence::ModernObservation {
            observation_year_ce: 2_021,
            quantities: quantities(),
        },
    );
    let decoded = HistoricalCorrectionDocument::deserialize(
        &document(vec![modern]).serialize().expect("modern JSON"),
    )
    .expect("explicit modern evidence");
    let record = &decoded.corrections[0];

    assert_eq!(record.evidence_year_ce(), Some(2_021));
    assert!(record.historical_quantities().is_none());
    assert_eq!(decoded.historical_corrections().count(), 0);
}

#[test]
fn complete_and_unknown_allocations_have_distinct_evidence() {
    let complete = HistoricalCorrection::from_whole_cell_allocation(
        3,
        4,
        &HydeAreaAllocation {
            land_area_square_meters: 100_000_000.0,
            lake_area_square_meters: 50_000_000.0,
            ocean_area_square_meters: 25_000_000.0,
            crop_area_square_kilometers: 25.0,
            grazing_area_square_kilometers: 10.0,
            population: 1_250.0,
            ..HydeAreaAllocation::default()
        },
    )
    .expect("complete allocation");
    let nodata = HistoricalCorrection::from_whole_cell_allocation(
        5,
        6,
        &HydeAreaAllocation {
            land_area_square_meters: 100_000_000.0,
            crop_area_square_kilometers: 25.0,
            grazing_area_square_kilometers: 10.0,
            population: 1_250.0,
            nodata_area_square_meters: 1.0,
            ..HydeAreaAllocation::default()
        },
    )
    .expect("unknown allocation");
    let outside = HistoricalCorrection::from_whole_cell_allocation(
        7,
        8,
        &HydeAreaAllocation {
            outside_area_square_meters: 1.0,
            ..HydeAreaAllocation::default()
        },
    )
    .expect("uncovered allocation");

    assert_eq!(complete.cell_x, 3);
    assert_eq!(complete.cell_y, 4);
    assert_eq!(
        complete
            .historical_quantities()
            .map(|value| value.population),
        Some(1_250.0)
    );
    assert!(matches!(
        nodata.evidence,
        HistoricalCorrectionEvidence::Unknown
    ));
    assert!(matches!(
        outside.evidence,
        HistoricalCorrectionEvidence::Unknown
    ));
    assert!(nodata.historical_quantities().is_none());
    assert!(outside.historical_quantities().is_none());
}

#[test]
fn whole_cell_conversion_rejects_excessive_or_nonfinite_quantities() {
    let cases = [
        HydeAreaAllocation {
            land_area_square_meters: 100_000_000.0,
            crop_area_square_kilometers: 101.0,
            ..HydeAreaAllocation::default()
        },
        HydeAreaAllocation {
            land_area_square_meters: 100_000_000.0,
            crop_area_square_kilometers: 60.0,
            grazing_area_square_kilometers: 50.0,
            ..HydeAreaAllocation::default()
        },
        HydeAreaAllocation {
            land_area_square_meters: f64::NAN,
            ..HydeAreaAllocation::default()
        },
    ];

    for allocation in cases {
        assert_correction_error(
            HistoricalCorrection::from_whole_cell_allocation(0, 0, &allocation).map(|_| ()),
            "whole-cell",
        );
    }
    assert_correction_error(
        HistoricalCorrection::from_whole_cell_allocation(
            0,
            0,
            &HydeAreaAllocation {
                land_area_square_meters: -0.0,
                ..HydeAreaAllocation::default()
            },
        )
        .map(|_| ()),
        "nonnegative",
    );
}

#[test]
fn validation_rejects_wrong_years_coordinates_duplicates_and_capacity() {
    let historical = HistoricalCorrectionEvidence::Circa600Hyde {
        quantities: quantities(),
    };
    let wrong_year = HistoricalCorrectionDocument {
        target_year_ce: 2_021,
        ..document(vec![record(0, 0, historical.clone())])
    }
    .validate()
    .map(|_| ());
    let out_of_bounds = HistoricalCorrectionDocument::new(
        2,
        vec![record(2, 0, HistoricalCorrectionEvidence::Unknown)],
    )
    .map(|_| ());
    let duplicate = HistoricalCorrectionDocument::new(
        2,
        vec![
            record(0, 0, historical.clone()),
            record(0, 0, HistoricalCorrectionEvidence::Unknown),
        ],
    )
    .map(|_| ());
    let excessive = HistoricalCorrectionDocument::new(
        2,
        vec![record(
            0,
            0,
            HistoricalCorrectionEvidence::Circa600Hyde {
                quantities: HydeWholeCellQuantities {
                    crop_area_square_kilometers: 101.0,
                    ..quantities()
                },
            },
        )],
    )
    .map(|_| ());
    let nonhistorical_excessive = HistoricalCorrectionDocument::new(
        2,
        vec![record(
            0,
            0,
            HistoricalCorrectionEvidence::FallbackEvidence {
                quantities: HydeWholeCellQuantities {
                    crop_area_square_kilometers: 101.0,
                    ..quantities()
                },
            },
        )],
    )
    .map(|_| ());

    assert_correction_error(wrong_year, "target year");
    assert_correction_error(out_of_bounds, "outside its grid");
    assert_correction_error(duplicate, "duplicated");
    assert_correction_error(excessive, "land capacity");
    assert_correction_error(nonhistorical_excessive, "land capacity");
}

#[test]
fn modern_year_schema_and_unknown_fields_fail_closed() {
    let modern_year = HistoricalCorrectionDocument::new(
        2,
        vec![record(
            0,
            0,
            HistoricalCorrectionEvidence::ModernObservation {
                observation_year_ce: 600,
                quantities: quantities(),
            },
        )],
    )
    .map(|_| ());
    assert_correction_error(modern_year, "modern bounds");

    for json in [
        br#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":"two","corrections":[]}"#.as_slice(),
        br#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":2}"#.as_slice(),
        br#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":2,"corrections":[{"cell_x":0,"cell_y":0,"evidence":{"kind":"unrecognized"}}]}"#.as_slice(),
        br#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":2,"corrections":[{"cell_x":0,"cell_y":0,"evidence":{"kind":"circa_600_hyde","quantities":{"land_area_square_meters":1.0,"crop_area_square_kilometers":0.0,"grazing_area_square_kilometers":0.0,"population":0.0},"observation_year_ce":2021}}]}"#.as_slice(),
        br#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":2,"corrections":[{"cell_x":0,"cell_y":1,"evidence":{"kind":"unknown"}},{"cell_x":0,"cell_y":0,"evidence":{"kind":"unknown"}}],"schema_version":1}"#.as_slice(),
        br#"not-json"#.as_slice(),
    ] {
        assert!(HistoricalCorrectionDocument::deserialize(json).is_err());
    }
}

#[test]
fn schema_version_one_is_compatible_and_other_versions_fail_closed() {
    assert_eq!(HISTORICAL_CORRECTION_SCHEMA_VERSION, 1);
    let fixture =
        br#"{"schema_version":1,"target_year_ce":600,"samples_per_axis":2,"corrections":[]}"#;
    assert_eq!(
        HistoricalCorrectionDocument::deserialize(fixture).expect("compatible version"),
        document(Vec::new())
    );

    for schema_version in [0, 2, u16::MAX] {
        let json = format!(
            "{{\"schema_version\":{schema_version},\"target_year_ce\":600,\"samples_per_axis\":2,\"corrections\":[]}}"
        );
        assert_correction_error(
            HistoricalCorrectionDocument::deserialize(json.as_bytes()).map(|_| ()),
            "schema version",
        );
    }
}

#[test]
fn byte_and_cell_limits_are_enforced() {
    {
        let oversized = vec![0_u8; MAX_HISTORICAL_CORRECTION_JSON_BYTES + 1];
        assert_correction_error(
            HistoricalCorrectionDocument::deserialize(&oversized).map(|_| ()),
            "byte limit",
        );
    }
    assert_correction_error(
        HistoricalCorrectionDocument::new(
            1,
            vec![record(0, 0, HistoricalCorrectionEvidence::Unknown)],
        )
        .map(|_| ()),
        "outside direct bounds",
    );

    let large_quantities = HydeWholeCellQuantities {
        land_area_square_meters: 1.0e308,
        ..quantities()
    };
    let corrections = (0..350_000)
        .map(|index| {
            record(
                (index % 1_024) as u16,
                (index / 1_024) as u16,
                HistoricalCorrectionEvidence::Circa600Hyde {
                    quantities: large_quantities.clone(),
                },
            )
        })
        .collect();
    let oversized_document = HistoricalCorrectionDocument {
        schema_version: HISTORICAL_CORRECTION_SCHEMA_VERSION,
        target_year_ce: HISTORICAL_CORRECTION_TARGET_YEAR_CE,
        samples_per_axis: MAX_HISTORICAL_CORRECTION_SAMPLES_PER_AXIS,
        corrections,
    };
    assert_correction_error(oversized_document.serialize().map(|_| ()), "byte limit");
}
