use super::*;
use std::collections::BTreeMap;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn bounded_role_ranges_preserve_reference_lookup_absence_and_replacement() {
    let mut ranges: RoleRanges = std::array::from_fn(|_| None);
    let mut reference = BTreeMap::new();
    let sources = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES);
    for source in sources.clone() {
        assert!((source.role as usize) < ROLE_SLOTS);
        assert_eq!(ranges[source.role as usize], None);
        for range in [0..source.frames as usize, 2..source.frames as usize + 2] {
            reference.insert(source.role, range.clone());
            ranges[source.role as usize] = Some(range);
            assert_eq!(
                ranges[source.role as usize].as_ref(),
                reference.get(&source.role)
            );
        }
    }
    for role in sources.map(|source| source.role).chain([AssetRole::Rock]) {
        assert_eq!(
            ranges.get(role as usize).and_then(Option::as_ref),
            reference.get(&role)
        );
    }
    assert_eq!(ranges[AssetRole::Rock as usize], None);
}

#[wasm_bindgen_test]
fn optional_source_absence_is_distinct_from_corrupt_present_frames() {
    let manifest: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
    assert!(!source_present(&manifest, "absent"));
    assert_eq!(source_frames(&manifest, "absent", 2), None);
    assert!(source_present(&manifest, "fixture"));
    assert_eq!(source_frames(&manifest, "fixture", 2), None);
}

#[wasm_bindgen_test]
fn source_indices_preserve_frame_order_and_reject_missing_or_duplicate_frames() {
    let mut value = manifest::fixture();
    let first = value["frames"][0].clone();
    value["frames"] = serde_json::json!([first.clone(), first.clone(), first.clone()]);
    for (index, frame) in [2, 0, 1].into_iter().enumerate() {
        value["frames"][index]["frame"] = frame.into();
    }
    let complete: Manifest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(source_frames(&complete, "fixture", 3), Some(vec![1, 2, 0]));
    assert_eq!(source_frames(&complete, "absent", 3), None);
    assert_eq!(source_frames(&complete, "fixture", 4), None);
    value["frames"][2]["frame"] = 0.into();
    let duplicate: Manifest = serde_json::from_value(value).unwrap();
    assert_eq!(source_frames(&duplicate, "fixture", 3), None);
}

#[wasm_bindgen_test]
fn native_forest_source_loads_only_ten_accents_from_the_full_hundred() {
    let mut value = manifest::fixture();
    let frame = value["frames"][0].clone();
    value["frames"] = serde_json::Value::Array(vec![frame; 100]);
    let selection = OPTIONAL_TERRAIN_SOURCES[0];
    let source = selection.manifest_source();
    for index in 0..100 {
        value["frames"][index]["frame"] = (index as u32).into();
        value["frames"][index]["source"] = source.clone().into();
    }
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    assert_eq!(
        source_frames(&manifest, &source, selection.frames),
        Some((0..10).collect())
    );
}

#[wasm_bindgen_test]
fn startup_index_sort_matches_std_at_catalogue_bound() {
    let bound = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(OPTIONAL_RESOURCE_SOURCES.iter())
        .chain(OPTIONAL_TERRAIN_SOURCES.iter())
        .map(|selection| selection.frames as usize)
        .sum::<usize>();
    for size in [0, 1, bound.saturating_sub(2), bound] {
        let mut shuffled = (0..size).collect::<Vec<_>>();
        let mut seed = 0x2a97_1845_u32;
        for index in (1..size).rev() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            shuffled.swap(index, seed as usize % (index + 1));
        }
        for input in [
            (0..size).collect::<Vec<_>>(),
            (0..size).rev().collect(),
            vec![17; size],
            shuffled,
        ] {
            let mut actual = input.clone();
            let mut expected = input;
            expected.sort_unstable();
            let mut comparisons = 0_usize;
            sort_indices(&mut actual, &mut |left, right| {
                comparisons += 1;
                left.cmp(right)
            });
            assert_eq!(actual, expected);
            assert!(comparisons <= size * size.saturating_sub(1) / 2);
        }
    }
    let mut value = manifest::fixture();
    let frame = value["frames"][0].clone();
    value["frames"] = serde_json::Value::Array(vec![frame; bound + 1]);
    let oversized: Manifest = serde_json::from_value(value).unwrap();
    assert_eq!(source_frames(&oversized, "fixture", 3), None);
}
