use super::*;

#[wasm_bindgen_test]
fn native_timelines_do_not_reserve_startup_pixels_or_collapse_distinct_cells() {
    let mut value = manifest::fixture();
    let mut native = value["frames"][0].clone();
    native["width"] = 2044.into();
    native["height"] = 2044.into();
    native["native"] = serde_json::json!({"chunk":0,"offset":0,"length":64});
    value["chunks"] =
        serde_json::json!([{"file":"frames-0000.bin","hash":"a".repeat(64),"bytes":64}]);
    for index in 1..1025 {
        native["frame"] = index.into();
        value["frames"].as_array_mut().unwrap().push(native.clone());
    }
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let selected: Vec<_> = (0..1025).collect();
    let places = plan(
        &manifest,
        &selected,
        &vec![AtlasDomain::Objects; selected.len()],
    )
    .unwrap();
    assert_eq!((places[0].width, places[0].height), (2, 4));
    assert!(places[1..].iter().all(|p| *p == Placement::default()));
    let mut invalid = manifest;
    invalid.frames[1].native.as_mut().unwrap().length = 65;
    assert!(
        plan(
            &invalid,
            &selected,
            &vec![AtlasDomain::Objects; selected.len()]
        )
        .is_err()
    );
}
