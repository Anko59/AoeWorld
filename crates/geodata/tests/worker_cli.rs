use aoe_geodata::GeneratedMap;
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aoe-worker-cli-{}-{nanos}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn request(&self) -> PathBuf {
        let path = self.0.join("request.json");
        fs::write(&path, serde_json::to_vec(&MapRequest::default()).unwrap()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(root: &Path, args: &[&str], environment: &[(&str, &str)], input: &[u8]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aoe-map-worker"));
    command
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in [
        "AOE_MAP_REQUEST",
        "AOE_MAP_PACKAGE",
        "AOE_MAP_SAMPLES",
        "AOE_MAP_DEM_RESOLUTION",
        "AOE_GEODATA_CACHE",
    ] {
        command.env_remove(key);
    }
    command.env("AOE_GEODATA_CACHE", root.join("cache"));
    for (key, value) in environment {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("worker fixture exceeded its deadline");
        }
        thread::sleep(Duration::from_millis(5));
    }
    child.wait_with_output().unwrap()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn failure(output: Output, message: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_estimates_and_labels_synthetic_sampling_without_source_acquisition() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let env = [("AOE_MAP_REQUEST", request.to_str().unwrap())];
    let estimate = success(run(&fixture.0, &["map-estimate"], &env, b""));
    assert!(estimate.is_object());
    let perf = success(run(&fixture.0, &["map-perf"], &env, b""));
    assert_eq!(perf["evidence_class"], "synthetic_fallback_sampling");
    assert_eq!(perf["chunks_sampled"], 5);
    assert!(perf["tiles_sampled"].as_u64().unwrap() > 0);
    assert!(
        !fixture.0.join("cache").exists(),
        "offline commands must not acquire data"
    );
}

#[test]
fn cli_rejects_invalid_requests_and_detail_options_before_network_access() {
    let fixture = Fixture::new();
    failure(run(&fixture.0, &["unknown"], &[], b""), "usage:");
    failure(
        run(&fixture.0, &["map-estimate", "extra"], &[], b""),
        "usage:",
    );
    failure(
        run(&fixture.0, &["map-estimate"], &[], b""),
        "AOE_MAP_REQUEST",
    );
    let request = fixture.request();
    let path = request.to_str().unwrap();
    let output = fixture.0.join("output");
    let common = [
        ("AOE_MAP_REQUEST", path),
        ("AOE_MAP_PACKAGE", output.to_str().unwrap()),
    ];
    for (key, value, expected) in [
        ("AOE_MAP_SAMPLES", "bad", "must be an integer"),
        ("AOE_MAP_DEM_RESOLUTION", "bad", "must be glo90 or glo30"),
        ("AOE_MAP_SAMPLES", "1", "2 through 4096"),
    ] {
        let mut env = common.to_vec();
        env.push((key, value));
        failure(
            run(&fixture.0, &["map-generate-detailed"], &env, b""),
            expected,
        );
    }
    failure(
        run(
            &fixture.0,
            &["map-generate"],
            &[("AOE_MAP_REQUEST", path)],
            b"",
        ),
        "AOE_MAP_PACKAGE",
    );
    fs::write(&request, b"{").unwrap();
    failure(
        run(&fixture.0, &["map-estimate"], &common, b""),
        "request.json",
    );
    fs::write(&request, vec![b' '; 65_537]).unwrap();
    failure(
        run(&fixture.0, &["map-estimate"], &common, b""),
        "byte input limit",
    );
    assert!(!fixture.0.join("cache").exists());
}

#[test]
fn verification_accepts_one_canonical_manifest_and_rejects_ambiguity_and_tampering() {
    let fixture = Fixture::new();
    let output = fixture.0.join("package");
    let generated = GeneratedMap {
        package: MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), vec![]).unwrap(),
        elevation_pages: vec![],
        water_pages: vec![],
        vegetation_pages: vec![],
        historical_land_use_pages: vec![],
        hydrology_evidence_pages: vec![],
        modern_land_cover_pages: vec![],
    };
    generated.write_directory(&output).unwrap();
    let hash = generated.package.content_hash_hex();
    let manifest = output.join(format!("{hash}.json"));
    for path in [&output, &manifest] {
        let result = run(
            &fixture.0,
            &["map-verify"],
            &[("AOE_MAP_PACKAGE", path.to_str().unwrap())],
            b"",
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains(&hash));
    }
    let second = output.join("other.json");
    fs::write(&second, b"{}").unwrap();
    failure(
        run(
            &fixture.0,
            &["map-verify"],
            &[("AOE_MAP_PACKAGE", output.to_str().unwrap())],
            b"",
        ),
        "exactly one",
    );
    fs::remove_file(second).unwrap();
    fs::write(&manifest, b"{}").unwrap();
    failure(
        run(
            &fixture.0,
            &["map-verify"],
            &[("AOE_MAP_PACKAGE", manifest.to_str().unwrap())],
            b"",
        ),
        "invalid",
    );
    fs::remove_file(manifest).unwrap();
    failure(
        run(
            &fixture.0,
            &["map-verify"],
            &[("AOE_MAP_PACKAGE", output.to_str().unwrap())],
            b"",
        ),
        "exactly one",
    );
}

#[test]
fn offline_cache_verification_reports_missing_source_identity() {
    let fixture = Fixture::new();
    let result = run(&fixture.0, &["verify"], &[], b"");
    failure(result, "etopo-2022-v1-60s-surface");
    assert!(fixture.0.join("cache").is_dir());
}

#[test]
fn worker_rejects_malformed_and_oversized_stdin_and_projects_valid_requests() {
    let fixture = Fixture::new();
    failure(run(&fixture.0, &[], &[], b"{"), "invalid worker request");
    failure(
        run(&fixture.0, &[], &[], &vec![b' '; 65_537]),
        "worker request exceeds",
    );
    let input = json!({"operation":"project_point", "center_latitude_e7":488500000, "center_longitude_e7":23500000, "longitude":2.35, "latitude":48.85});
    let projected = success(run(
        &fixture.0,
        &[],
        &[],
        &serde_json::to_vec(&input).unwrap(),
    ));
    assert_eq!(projected["east_meters"], 0);
    assert_eq!(projected["north_meters"], 0);
    let input = json!({"operation":"project_footprint", "request":MapRequest::default(), "samples_per_edge":4});
    let footprint = success(run(
        &fixture.0,
        &[],
        &[],
        &serde_json::to_vec(&input).unwrap(),
    ));
    assert!(!footprint["points"].as_array().unwrap().is_empty());
    assert!(footprint["distortion"].is_object());
    let input = json!({"operation":"list_overview_sources"});
    let sources = success(run(
        &fixture.0,
        &[],
        &[],
        &serde_json::to_vec(&input).unwrap(),
    ));
    assert_eq!(sources["sources"][0]["id"], "etopo-2022-v1-60s-surface");
}

#[test]
fn worker_inspects_and_prepares_real_local_raster() {
    let fixture = Fixture::new();
    let path = fixture.0.join("elevation.tif");
    let driver = gdal::DriverManager::get_driver_by_name("GTiff").unwrap();
    let mut raster = driver
        .create_with_band_type::<f64, _>(&path, 256, 256, 1)
        .unwrap();
    raster
        .set_geo_transform(&[1.0, 0.01, 0.0, 50.5, 0.0, -0.01])
        .unwrap();
    raster
        .set_spatial_ref(&gdal::spatial_ref::SpatialRef::from_epsg(4326).unwrap())
        .unwrap();
    raster
        .rasterband(1)
        .unwrap()
        .write(
            (0, 0),
            (256, 256),
            &mut gdal::raster::Buffer::new((256, 256), vec![25.0; 256 * 256]),
        )
        .unwrap();
    raster.flush_cache().unwrap();
    drop(raster);
    let input = json!({"operation":"inspect_raster", "path":path});
    let dimensions = success(run(
        &fixture.0,
        &[],
        &[],
        &serde_json::to_vec(&input).unwrap(),
    ));
    assert_eq!(dimensions["width"], 256);
    assert_eq!(dimensions["height"], 256);
    let input = json!({"operation":"prepare_elevation", "path":path, "request":MapRequest::default(), "samples_per_axis":2});
    let prepared = success(run(
        &fixture.0,
        &[],
        &[],
        &serde_json::to_vec(&input).unwrap(),
    ));
    assert_eq!(
        prepared["pages"][0]["geographic_height_centimeters"],
        json!([2500, 2500, 2500, 2500])
    );
}
