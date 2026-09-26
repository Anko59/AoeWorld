#[cfg(test)]
use super::{ElevationRange, WaterEvidence, copy_tree, qualify_case, valid_hash};
#[cfg(test)]
use std::fs;

#[test]
fn qualification_requires_the_region_specific_water_and_relief() {
    let no_water = WaterEvidence {
        sample_count: 10,
        ocean_nonzero_samples: 0,
        inland_nonzero_samples: 0,
        ocean_coverage_percent_sum: 0,
        inland_coverage_percent_sum: 0,
    };
    let relief = ElevationRange {
        minimum_centimeters: 0,
        maximum_centimeters: 150_000,
        level_zero_samples: 4,
    };
    assert!(qualify_case("river_lake", "inland", Some(&no_water), &relief).is_err());
    assert!(qualify_case("nile_delta_coast", "ocean", Some(&no_water), &relief).is_err());
    assert!(qualify_case("alpine_relief", "relief", None, &relief).is_ok());
    assert!(
        qualify_case(
            "alpine_relief",
            "relief",
            None,
            &ElevationRange {
                maximum_centimeters: 149_999,
                ..relief
            }
        )
        .is_err()
    );
}

#[test]
fn only_lowercase_sha256_content_hashes_are_accepted() {
    assert!(valid_hash(&"a".repeat(64)));
    assert!(!valid_hash(&"A".repeat(64)));
    assert!(!valid_hash(&"a".repeat(63)));
}

#[test]
fn source_pages_stage_under_a_new_nested_package_directory() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let source = temporary.path().join("source");
    fs::create_dir_all(source.join("elevation")).expect("source page directory");
    fs::write(source.join("elevation/0-0-0.json"), b"page").expect("source page");
    let target = temporary.path().join("staging/pages/hash/pages/hash");

    copy_tree(&source, &target).expect("copy nested page tree");

    assert_eq!(
        fs::read(target.join("elevation/0-0-0.json")).expect("staged page"),
        b"page"
    );
}
