use super::Result;
use aoe_map::{
    CompactChunk, ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyObservation,
    HydrologyWaterPolicy, MAP_SCHEMA_VERSION, MapChunkGenerator, MapPackage, MapRequest,
    ModernLandCoverPage, PreparedEnvironment, PyramidLevel, WORLD_COVER_OBSERVATION_YEAR,
    ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
};
use serde::Serialize;
use std::{
    fs,
    io::{ErrorKind, Write},
    path::Path,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Seed {
    pub target: &'static str,
    pub name: String,
    pub path: String,
    pub bytes: usize,
    pub blake3_hex: String,
}

impl Seed {
    pub(super) fn new(target: &'static str, name: String, path: String, bytes: &[u8]) -> Self {
        Self {
            target,
            name,
            path,
            bytes: bytes.len(),
            blake3_hex: blake3_hex(bytes),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct SeedInventory {
    pub prepared_seeds: Vec<Seed>,
    pub verified_legacy_seeds: Vec<Seed>,
}

struct LegacySeed {
    target: &'static str,
    name: &'static str,
    bytes: &'static [u8],
}

const LEGACY_SEEDS: [LegacySeed; 4] = [
    LegacySeed {
        target: "drs",
        name: "one-entry.drs",
        bytes: include_bytes!("../../../../fuzz/corpus/drs/one-entry.drs"),
    },
    LegacySeed {
        target: "manifest",
        name: "minimal.json",
        bytes: include_bytes!("../../../../fuzz/corpus/manifest/minimal.json"),
    },
    LegacySeed {
        target: "palette",
        name: "jasc.pal",
        bytes: include_bytes!("../../../../fuzz/corpus/palette/jasc.pal"),
    },
    LegacySeed {
        target: "slp",
        name: "two-pixels.slp",
        bytes: include_bytes!("../../../../fuzz/corpus/slp/two-pixels.slp"),
    },
];

/// Valid seeds reach validation and roundtrip paths from the first smoke run.
/// Generated inputs and discovered mutations stay in the ignored corpus tree.
pub(super) fn prepare(root: &Path) -> Result<SeedInventory> {
    let mut prepared_seeds = Vec::new();
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())?;
    prepared_seeds.push(write(
        root,
        "map_package",
        "package",
        &serde_json::to_vec(&package)?,
    )?);
    prepared_seeds.push(write(
        root,
        "map_package",
        "request",
        &serde_json::to_vec(&package.request)?,
    )?);
    let hydrology = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::Lake as u8; 4],
        method: vec![HydrologyEvidenceMethod::HydroLakesExtent as u8; 4],
        water_model: None,
    };
    let land_cover = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![10; 4],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [1; 32],
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [2; 32],
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: Some(HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&hydrology))?,
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(
                std::slice::from_ref(&land_cover),
            )?,
            water_model: None,
        }),
    };
    let typed = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        MapRequest::default(),
        Vec::new(),
        Default::default(),
        Default::default(),
        environment,
    )?;
    prepared_seeds.push(write(
        root,
        "map_package",
        "typed-package",
        &serde_json::to_vec(&typed)?,
    )?);
    prepared_seeds.push(write(
        root,
        "map_package",
        "schema8-default",
        include_bytes!("../../../map/tests/fixtures/schema8-default-package.json"),
    )?);
    for (name, field) in [
        (
            "elevation",
            serde_json::json!({"geographic_height_centimeters": [120]}),
        ),
        (
            "water",
            serde_json::json!({"ocean_coverage_percent": [0], "inland_coverage_percent": [100]}),
        ),
        (
            "vegetation",
            serde_json::json!({"potential_biome_class": [1]}),
        ),
        (
            "history",
            serde_json::json!({"crop_percent": [20], "grazing_percent": [30], "population_pressure_per_square_kilometer": [10]}),
        ),
    ] {
        let mut page = serde_json::json!({"level": 0, "x": 0, "y": 0, "width": 1, "height": 1});
        let object = page.as_object_mut().ok_or("page seed is not an object")?;
        object.extend(
            field
                .as_object()
                .ok_or("page fields are not an object")?
                .clone(),
        );
        prepared_seeds.push(write(
            root,
            "environment_page",
            name,
            &serde_json::to_vec(&page)?,
        )?);
    }
    for name in ["hydrology-evidence", "schema9-typed-hydrology"] {
        prepared_seeds.push(write(
            root,
            "environment_page",
            name,
            &serde_json::to_vec(&hydrology)?,
        )?);
    }
    for name in ["modern-land-cover", "schema9-modern-land-cover"] {
        prepared_seeds.push(write(
            root,
            "environment_page",
            name,
            &serde_json::to_vec(&land_cover)?,
        )?);
    }
    let chunk = MapChunkGenerator::new([3; 32], 7, 32).chunk(0, 0)?;
    prepared_seeds.push(write(
        root,
        "map_chunk",
        "full-chunk",
        &chunk_bytes(&chunk)?,
    )?);
    let mut observed_tile = chunk.tiles[0];
    observed_tile.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::Land,
        method: HydrologyEvidenceMethod::WorldCoverClass,
    });
    observed_tile.modern_land_cover_class = Some(10);
    let observed = aoe_map::Chunk {
        x: 0,
        y: 0,
        tiles: vec![observed_tile],
        resources: Vec::new(),
    };
    prepared_seeds.push(write(
        root,
        "map_chunk",
        "typed-v2",
        &chunk_bytes(&observed)?,
    )?);
    prepared_seeds.push(write(root, "map_chunk", "invalid-hex", b"not-hex-payload")?);
    prepared_seeds.push(write(root, "map_chunk", "empty-legacy", &[1, 0, 0, 0, 0])?);
    prepared_seeds.push(write(
        root,
        "map_chunk",
        "one-tile-v1",
        &[
            1, 1, 0, 0, 0, 2, 0xde, 0xff, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9, 0xff,
            0xa9, 0xff, 0x65, 0xc3, 0x16, 0x00,
        ],
    )?);
    Ok(SeedInventory {
        prepared_seeds,
        verified_legacy_seeds: verify_legacy_seeds(root)?,
    })
}

fn chunk_bytes(chunk: &aoe_map::Chunk) -> Result<Vec<u8>> {
    let hex = CompactChunk::encode(chunk)?.payload_hex;
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect()
}
fn write(root: &Path, target: &'static str, name: &str, bytes: &[u8]) -> Result<Seed> {
    let directory = root.join("fuzz/corpus").join(target);
    fs::create_dir_all(&directory)?;
    let preferred = directory.join(name);
    let retained_name = if retain(&preferred, bytes)? {
        name.to_owned()
    } else {
        // Existing bytes are never overwritten. A changed generator gets a
        // deterministic content-addressed name while the earlier seed stays.
        let digest = blake3_hex(bytes);
        let unique_name = format!("{name}.{digest}");
        if !retain(&directory.join(&unique_name), bytes)? {
            return Err("content-addressed fuzz seed collision".into());
        }
        unique_name
    };
    let path = format!("fuzz/corpus/{target}/{retained_name}");
    Ok(Seed::new(target, retained_name, path, bytes))
}

fn retain(path: &Path, bytes: &[u8]) -> Result<bool> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => Ok(true),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(bytes)?;
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) fn verify_legacy_seeds(root: &Path) -> Result<Vec<Seed>> {
    LEGACY_SEEDS
        .iter()
        .map(|legacy| {
            let path = format!("fuzz/corpus/{}/{}", legacy.target, legacy.name);
            fs::create_dir_all(root.join("fuzz/corpus").join(legacy.target))?;
            if !retain(&root.join(&path), legacy.bytes)? {
                return Err(format!("legacy fuzz seed changed; refusing to replace {path}").into());
            }
            Ok(Seed::new(
                legacy.target,
                legacy.name.to_owned(),
                path,
                legacy.bytes,
            ))
        })
        .collect()
}

pub(super) fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeds_reach_valid_parsers_and_preserve_discovered_inputs() {
        let root = tempfile::tempdir().unwrap();
        let inventory = prepare(root.path()).unwrap();
        let corpus = root.path().join("fuzz/corpus");
        let package: MapPackage =
            serde_json::from_slice(&fs::read(corpus.join("map_package/package")).unwrap()).unwrap();
        package.validate().unwrap();
        for name in ["typed-package", "schema8-default"] {
            let package: MapPackage =
                serde_json::from_slice(&fs::read(corpus.join("map_package").join(name)).unwrap())
                    .unwrap();
            package.validate().unwrap();
            if name == "typed-package" {
                assert_eq!(package.schema_version, MAP_SCHEMA_VERSION);
                assert_eq!(package.schema_version, 9);
                assert!(package.environment.hydrology_evidence.is_some());
            }
        }
        macro_rules! check {
            ($name:expr, $kind:ty) => {
                let page: $kind = serde_json::from_slice(
                    &fs::read(corpus.join(concat!("environment_page/", $name))).unwrap(),
                )
                .unwrap();
                page.validate().unwrap();
            };
        }
        check!("elevation", aoe_map::ElevationPage);
        check!("water", aoe_map::WaterPage);
        check!("vegetation", aoe_map::PotentialBiomePage);
        check!("history", aoe_map::HistoricalLandUsePage);
        check!("hydrology-evidence", aoe_map::HydrologyEvidencePage);
        check!("schema9-typed-hydrology", aoe_map::HydrologyEvidencePage);
        check!("modern-land-cover", aoe_map::ModernLandCoverPage);
        check!("schema9-modern-land-cover", aoe_map::ModernLandCoverPage);
        let invalid = fs::read(corpus.join("map_chunk/invalid-hex")).unwrap();
        assert_eq!(
            CompactChunk {
                x: 0,
                y: 0,
                payload_hex: String::from_utf8_lossy(&invalid).into_owned(),
            }
            .decode(),
            Err(aoe_map::CompactChunkError::InvalidHex)
        );
        for path in [
            "fuzz/corpus/environment_page/schema9-typed-hydrology",
            "fuzz/corpus/environment_page/schema9-modern-land-cover",
            "fuzz/corpus/map_chunk/one-tile-v1",
        ] {
            assert!(
                inventory
                    .prepared_seeds
                    .iter()
                    .any(|seed| seed.path == path)
            );
        }
        for legacy in &LEGACY_SEEDS {
            let path = format!("fuzz/corpus/{}/{}", legacy.target, legacy.name);
            assert_eq!(fs::read(root.path().join(&path)).unwrap(), legacy.bytes);
            let seed = inventory
                .verified_legacy_seeds
                .iter()
                .find(|seed| seed.path == path)
                .expect("verified legacy seed");
            assert_eq!(
                seed.blake3_hex,
                blake3::hash(legacy.bytes).to_hex().to_string()
            );
        }
        let hydrology_alias = fs::read(corpus.join("environment_page/hydrology-evidence")).unwrap();
        let hydrology_schema9 =
            fs::read(corpus.join("environment_page/schema9-typed-hydrology")).unwrap();
        let land_cover_alias = fs::read(corpus.join("environment_page/modern-land-cover")).unwrap();
        let land_cover_schema9 =
            fs::read(corpus.join("environment_page/schema9-modern-land-cover")).unwrap();
        assert_eq!(hydrology_alias, hydrology_schema9);
        assert_eq!(land_cover_alias, land_cover_schema9);
        assert_ne!(hydrology_schema9, land_cover_schema9);
        let hydrology: aoe_map::HydrologyEvidencePage =
            serde_json::from_slice(&hydrology_schema9).unwrap();
        let land_cover: aoe_map::ModernLandCoverPage =
            serde_json::from_slice(&land_cover_schema9).unwrap();
        assert_ne!(
            hydrology.content_hash().unwrap(),
            land_cover.content_hash().unwrap()
        );
        for name in ["one-tile-v1", "typed-v2"] {
            let bytes = fs::read(corpus.join("map_chunk").join(name)).unwrap();
            assert_eq!(bytes[0], if name == "one-tile-v1" { 1 } else { 2 });
            let payload_hex = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            let chunk = CompactChunk {
                x: 0,
                y: 0,
                payload_hex,
            }
            .decode()
            .unwrap();
            assert_eq!(chunk.tiles.len(), 1);
            assert_eq!(
                chunk.tiles[0].modern_land_cover_class,
                if name == "typed-v2" { Some(10) } else { None },
            );
        }
        fs::write(corpus.join("map_chunk/discovered"), b"keep").unwrap();
        prepare(root.path()).unwrap();
        assert_eq!(
            fs::read(corpus.join("map_chunk/discovered")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn fixed_seed_names_retain_existing_bytes_and_changed_names_stay_stable() {
        let root = tempfile::tempdir().unwrap();
        let first = prepare(root.path()).unwrap();
        let corpus = root.path().join("fuzz/corpus");
        let legacy = corpus.join("map_chunk/one-tile-v1");
        let generated = corpus.join("map_package/package");
        let captured_legacy = fs::read(&legacy).unwrap();
        assert_eq!(captured_legacy.first(), Some(&1));
        fs::write(&legacy, b"retained legacy seed").unwrap();
        fs::write(&generated, b"retained generated seed").unwrap();

        let second = prepare(root.path()).unwrap();
        assert_eq!(fs::read(&legacy).unwrap(), b"retained legacy seed");
        assert_eq!(fs::read(&generated).unwrap(), b"retained generated seed");
        for (name, preferred, inventory) in [
            ("one-tile-v1", legacy, &first),
            ("package", generated, &first),
        ] {
            let retained = second
                .prepared_seeds
                .iter()
                .find(|seed| {
                    seed.target
                        == if name == "package" {
                            "map_package"
                        } else {
                            "map_chunk"
                        }
                        && seed.name.starts_with(&format!("{name}."))
                })
                .expect("content-addressed replacement seed");
            assert_ne!(preferred, root.path().join(&retained.path));
            assert!(
                inventory
                    .prepared_seeds
                    .iter()
                    .all(|seed| seed.path != retained.path)
            );
            assert_eq!(
                retained.bytes,
                fs::read(root.path().join(&retained.path)).unwrap().len()
            );
        }

        let count = fs::read_dir(corpus.join("map_chunk")).unwrap().count()
            + fs::read_dir(corpus.join("map_package")).unwrap().count();
        let third = prepare(root.path()).unwrap();
        assert_eq!(second, third);
        assert_eq!(
            count,
            fs::read_dir(corpus.join("map_chunk")).unwrap().count()
                + fs::read_dir(corpus.join("map_package")).unwrap().count()
        );
    }

    #[test]
    fn changed_legacy_blob_is_preserved_and_blocks_the_fuzz_campaign() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("fuzz/corpus/drs");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("one-entry.drs"), b"preserved older blob").unwrap();
        assert!(verify_legacy_seeds(root.path()).is_err());
        assert_eq!(
            fs::read(directory.join("one-entry.drs")).unwrap(),
            b"preserved older blob"
        );
    }
}
