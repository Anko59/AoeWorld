use super::*;
use crate::{DownloadPolicy, SourceCache, SourceLock};
use sha2::{Digest, Sha256};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_CACHE: AtomicU64 = AtomicU64::new(0);

#[test]
fn catalog_requires_the_pinned_raster_and_class_lookup() {
    let sources = parse_potential_biome_sources(
            r#"{"files":[
                {"key":"pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif","size":210668848,"checksum":"md5:e67c4778153fe5dcd9c637f4846e2f03","links":{"self":"https://zenodo.org/file.tif"}},
                {"key":"pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv","size":2968,"checksum":"md5:874f169f966e039935108bc366773f80","links":{"self":"https://zenodo.org/file.csv"}}
            ]}"#,
        )
        .expect("catalog");
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].bytes, 210_668_848);
    assert_eq!(
        sources[1].expected_checksum,
        ExpectedChecksum::Md5([
            0x87, 0x4f, 0x16, 0x9f, 0x96, 0x6e, 0x03, 0x99, 0x35, 0x10, 0x8b, 0xc3, 0x66, 0x77,
            0x3f, 0x80
        ])
    );
}

#[test]
fn overview_source_has_a_reviewed_sha256_checksum() {
    let source = etopo_2022_60s_surface();
    assert_eq!(source.bytes, 465_969_062);
    assert!(matches!(
        source.expected_checksum,
        ExpectedChecksum::Sha256([0x9d, 0x27, 0xd4, 0xb8, ..])
    ));
    assert_eq!(
        source.cache_lock().expect("cache lock").sha256,
        "9d27d4b8ea8e76977e2988bca667d7c8fa68b927355feffcddd6b4875a7fd08e"
    );
}

#[test]
fn coastline_fallback_is_a_pinned_independent_source() {
    let source = natural_earth_10m_land();
    assert_eq!(source.provider, Provider::NaturalEarth);
    assert_eq!(source.bytes, 3_269_070);
    assert_eq!(
        source.cache_lock().expect("cache lock").sha256,
        "e547d749445eaa0964aba76738090ec88f5e63c4585122170f98c67a7ea922dc"
    );
}

#[test]
fn catalog_requires_hyde_baseline_supplementary_and_readme() {
    let sources = parse_hyde_sources(
            r#"{"data":{"latestVersion":{"files":[
                {"dataFile":{"id":5490328,"filename":"HYDE3_2_1-baseline.zip","filesize":5339653974,"checksum":{"type":"SHA-1","value":"0d0e4ff97deb59664ce6c34dfdeeafa08e487d20"}}},
                {"dataFile":{"id":5490327,"filename":"HYDE3_2_1-general_supplementary.zip","filesize":23585889,"checksum":{"type":"SHA-1","value":"3cfe98d21e70c9ce478460c7265962f1bb2b6aab"}}},
                {"dataFile":{"id":5396388,"filename":"readme_release_HYDE3.2.1.txt","filesize":8826,"checksum":{"type":"SHA-1","value":"821309ce6035c68033c6e5b3522982cd515bd111"}}}
            ]}}}"#,
        )
        .expect("HYDE catalog");
    assert_eq!(sources.len(), 3);
    assert_eq!(sources[0].bytes, 5_339_653_974);
    assert!(matches!(
        sources[0].expected_checksum,
        ExpectedChecksum::Sha1(_)
    ));
    assert!(
        sources
            .iter()
            .all(|source| source.provider == Provider::Dans)
    );
}

#[test]
fn worldcover_selection_keeps_every_closed_boundary_tile() {
    let tiles = worldcover_tile_ids(47.9, 49.1, 2.9, 3.1).expect("tiles");
    assert_eq!(tiles, vec![(45, 0), (45, 3), (48, 0), (48, 3)]);
    assert!(worldcover_tile_ids(47.0, 49.0, 2.0, 3.0).is_ok());
    assert!(worldcover_tile_ids(47.0, 49.0, 3.0, 3.0).is_err());
    assert!(worldcover_tile_ids(47.0, 49.0, 179.0, -179.0).is_err());
    assert!(matches!(
        worldcover_tile_ids(-89.0, 82.0, -179.0, 179.0),
        Err(SourceCatalogError::TooManyWorldCoverTiles)
    ));
}

#[test]
fn hydrology_vector_catalog_is_separate_from_dem_budget() {
    let sources = hydrology_vector_sources();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].bytes + sources[1].bytes, 830_168_731);
    assert_eq!(
        sources[0].cache_lock().expect("HydroLAKES lock").sha256,
        "1c1303a4882c597b769f4a2beae6c72804c52ad418a0b4078817cf1062116643"
    );
    assert_eq!(
        sources[1].cache_lock().expect("HydroRIVERS lock").sha256,
        "500da7d36ceee0aa4c82dd625f5c53d80ba2f7ac14ad022cd3a6601bf55068e5"
    );
    assert!(
        sources
            .iter()
            .all(|source| source.provider == Provider::HydroSheds)
    );
    assert!(sources.iter().map(|source| source.bytes).sum::<u64>() < MAX_HYDROLOGY_DOWNLOAD_BYTES);
}

#[test]
fn worldcover_trust_on_first_use_is_not_a_checksum_match() {
    let policy = ExpectedChecksum::Sha256OnFirstAcquisition;
    let sha256 = [0x5a; 32];
    let sha1 = [0x11; 20];
    let md5 = [0x22; 16];
    assert!(!policy.matches(&sha256, &sha1, &md5));
    let name = "ESA_WorldCover_10m_2021_v200_N48E000_Map.tif";
    let source = KnownSource {
        id: format!("worldcover-2021-v200:{name}"),
        provider: Provider::EsaWorldCover,
        release: "ESA WorldCover 2021 v200".to_owned(),
        url: format!("{WORLD_COVER_BASE_URL}/{name}"),
        bytes: 1,
        expected_checksum: policy,
        native_resolution: "10 meters".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "not applicable".to_owned(),
        license_reference: "CC BY 4.0".to_owned(),
    };
    assert!(source.accepts_acquired_bytes(&sha256, &sha1, &md5));
    let mut redirected = source.clone();
    redirected.url = "https://esa-worldcover.s3.eu-central-1.amazonaws.com/other.tif".to_owned();
    assert!(!redirected.has_valid_acquisition_policy());
    assert!(!redirected.accepts_acquired_bytes(&sha256, &sha1, &md5));
    let mut non_worldcover = source;
    non_worldcover.provider = Provider::HydroSheds;
    assert!(!non_worldcover.has_valid_acquisition_policy());
    assert!(!non_worldcover.accepts_acquired_bytes(&sha256, &sha1, &md5));
}

#[test]
fn cached_worldcover_plan_uses_verified_size_without_provider_head() {
    let serial = NEXT_CACHE.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("aoe-worldcover-cache-{serial}"));
    let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
    let name = "ESA_WorldCover_10m_2021_v200_N48E000_Map.tif";
    let id = format!("worldcover-2021-v200:{name}");
    let lock = SourceLock {
        id: id.clone(),
        provider: Provider::EsaWorldCover,
        release: "ESA WorldCover 2021 v200".to_owned(),
        url: format!("{WORLD_COVER_BASE_URL}/{name}"),
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
        bytes: 3,
        native_resolution: "10 meters".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "not applicable".to_owned(),
        license_reference: "CC BY 4.0; ESA WorldCover attribution required".to_owned(),
    };
    fs::write(cache.object_path(&lock).expect("object path"), b"abc").expect("object");
    let known_name = format!("{:x}.json", Sha256::digest(id.as_bytes()));
    let known_path = root.join("known").join(&known_name);
    fs::write(
        &known_path,
        serde_json::to_vec(&lock).expect("known lock JSON"),
    )
    .expect("known lock");

    let sources = worldcover_sources_for_bounds_cached(48.1, 48.2, 1.1, 1.2, &cache)
        .expect("cached WorldCover metadata");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].bytes, 3);
    assert!(cache.known_lock(&id).expect("verified lock").is_some());
    let mut tampered = lock;
    tampered.release = "unexpected release".to_owned();
    fs::write(
        known_path,
        serde_json::to_vec(&tampered).expect("tampered lock JSON"),
    )
    .expect("tampered known lock");
    assert!(matches!(
        worldcover_sources_for_bounds_cached(48.1, 48.2, 1.1, 1.2, &cache),
        Err(SourceCatalogError::Cache(_))
    ));
    fs::remove_dir_all(root).expect("remove cache");
}
