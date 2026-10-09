use super::*;

#[test]
fn schema_ten_rejects_unknown_semantics_at_every_package_boundary() {
    let landscape = package(DetailProfile::LandscapeV2, true);
    for pointer in [
        "",
        "/request",
        "/request/compression",
        "/estimate",
        "/projection",
        "/provenance",
        "/source_locks/0",
        "/environment",
        "/environment/elevation",
        "/environment/elevation/levels/0",
        "/environment/hydrology_evidence",
        "/environment/hydrology_evidence/water_model",
        "/environment/hydrology_evidence/water_model/correction_document",
        "/environment/hydrology_evidence/water_model/correction_document/request",
        "/environment/hydrology_evidence/water_model/correction_document/request/compression",
    ] {
        let mut value = serde_json::to_value(&landscape).unwrap();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unrecognized_semantics".into(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<MapPackage>(value).is_err(),
            "unknown semantics accepted at {pointer}"
        );
    }
}

#[test]
fn legacy_permissive_fields_and_missing_defaults_remain_parseable() {
    let original = package(DetailProfile::StandardV1, false);
    for pointer in [
        "",
        "/request",
        "/request/compression",
        "/estimate",
        "/projection",
        "/provenance",
        "/source_locks/0",
        "/environment",
        "/environment/elevation",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("legacy_ignored_field".into(), serde_json::json!(true));
        assert_eq!(
            serde_json::from_value::<MapPackage>(value).unwrap(),
            original
        );
    }
    let legacy = MapPackage::with_generation_recipe(
        9,
        3,
        request(DetailProfile::StandardV1),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment::default(),
    )
    .unwrap();
    let mut value = serde_json::to_value(&legacy).unwrap();
    assert!(
        value
            .as_object_mut()
            .unwrap()
            .remove("generation_recipe_version")
            .is_none()
    );
    value["request"]
        .as_object_mut()
        .unwrap()
        .remove("detail_profile");
    value["request"]
        .as_object_mut()
        .unwrap()
        .remove("reconstruction_profile");
    let decoded: MapPackage = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, legacy);
    decoded.validate().unwrap();
    let mut unsupported = serde_json::to_value(&original).unwrap();
    unsupported["schema_version"] = 77.into();
    let parsed: MapPackage = serde_json::from_value(unsupported).unwrap();
    assert!(parsed.validate().is_err());
    for key in ["generation_recipe_version", "detail_profile"] {
        let mut value = serde_json::to_value(package(DetailProfile::LandscapeV2, false)).unwrap();
        if key == "detail_profile" {
            value["request"].as_object_mut().unwrap().remove(key);
        } else {
            value.as_object_mut().unwrap().remove(key);
        }
        assert!(serde_json::from_value::<MapPackage>(value).is_err());
    }
}

#[test]
fn duplicate_known_package_fields_never_collapse_before_strict_or_legacy_deserialization() {
    for profile in [DetailProfile::StandardV1, DetailProfile::LandscapeV2] {
        let value = serde_json::to_string(&package(profile, false)).unwrap();
        let duplicated = format!(
            "{{\"schema_version\":{},{}",
            if profile == DetailProfile::StandardV1 {
                9
            } else {
                10
            },
            &value[1..]
        );
        assert!(serde_json::from_str::<MapPackage>(&duplicated).is_err());
    }
}
