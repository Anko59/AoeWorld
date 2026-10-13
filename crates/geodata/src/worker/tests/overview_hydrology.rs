use super::*;

#[test]
fn preflight_rejects_vector_axis_mismatch_before_acquisition() {
    let request = MapRequest::default();
    let cancelled = AtomicBool::new(false);
    let axes = OverviewFieldAxes::LANDSCAPE;
    let vectors = OverviewHydrologyMode::Vectors;
    assert!(
        preflight(
            vectors,
            request,
            VECTOR_SAMPLES_PER_AXIS,
            axes,
            None,
            &cancelled
        )
        .is_ok()
    );
    assert!(matches!(
        preflight(vectors, request, 128, axes, None, &cancelled),
        Err(GeodataError::Preparation(message)) if message.contains("samples per axis")
    ));
}

#[test]
fn named_lock_counts_match_the_documented_source_lists() {
    let biome_sources = crate::parse_potential_biome_sources(
        r#"{"files":[
            {"key":"pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif","size":210668848,"checksum":"md5:e67c4778153fe5dcd9c637f4846e2f03","links":{"self":"https://zenodo.org/file.tif"}},
            {"key":"pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv","size":2968,"checksum":"md5:874f169f966e039935108bc366773f80","links":{"self":"https://zenodo.org/file.csv"}}
        ]}"#,
    )
    .expect("recorded Zenodo record payload");
    let hyde_catalog = crate::parse_hyde_sources(
        r#"{"data":{"latestVersion":{"files":[
            {"dataFile":{"id":5490328,"filename":"HYDE3_2_1-baseline.zip","filesize":5339653974,"checksum":{"type":"SHA-1","value":"0d0e4ff97deb59664ce6c34dfdeeafa08e487d20"}}},
            {"dataFile":{"id":5490327,"filename":"HYDE3_2_1-general_supplementary.zip","filesize":23585889,"checksum":{"type":"SHA-1","value":"3cfe98d21e70c9ce478460c7265962f1bb2b6aab"}}},
            {"dataFile":{"id":5396388,"filename":"readme_release_HYDE3.2.1.txt","filesize":8826,"checksum":{"type":"SHA-1","value":"821309ce6035c68033c6e5b3522982cd515bd111"}}}
        ]}}}"#,
    )
    .expect("recorded HYDE listing payload");
    assert_eq!(
        crate::overview_sources_from_parsed_catalogs(biome_sources, hyde_catalog).len(),
        LANDSCAPE_OVERVIEW_SOURCE_LOCKS
    );
    assert_eq!(
        crate::hydrology_vector_sources().len(),
        VECTOR_HYDROLOGY_SOURCE_LOCKS
    );
}

#[test]
fn automatic_preflight_covers_rivers_or_explicitly_marks_the_global_overview() {
    for (request, covered) in [
        (MapRequest::default(), true),
        (
            MapRequest {
                center_latitude_e7: -178_000_000,
                center_longitude_e7: 1_798_000_000,
                ..MapRequest::default()
            },
            false,
        ),
        (
            MapRequest {
                center_latitude_e7: 480_000_000,
                center_longitude_e7: -1_000_000_000,
                ..MapRequest::default()
            },
            false,
        ),
    ] {
        let plan = preflight(
            OverviewHydrologyMode::Automatic,
            request,
            1024,
            OverviewFieldAxes::LANDSCAPE,
            None,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(plan.has_vectors(), covered);
        if !covered {
            let package = plan
                .mark_package(MapPackage::new(1, request, Vec::new()).unwrap())
                .unwrap();
            assert_eq!(
                serde_json::to_value(&package).unwrap()["preparation_notes"],
                serde_json::json!(["outside_river_coverage"])
            );
            assert!(package.environment.hydrology_evidence.is_none());
            package.validate().unwrap();
            let bytes = serde_json::to_vec(&package).unwrap();
            let decoded: MapPackage = serde_json::from_slice(&bytes).unwrap();
            decoded.validate().unwrap();
            assert!(
                matches!(preflight(OverviewHydrologyMode::Vectors,request,1024,OverviewFieldAxes::LANDSCAPE,None,&AtomicBool::new(false)),Err(GeodataError::Preparation(message)) if message.contains("outside the pinned river dataset extent"))
            );
        }
    }
}
