use super::Result;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, HydrologyEvidenceIndex, HydrologyEvidenceMethod,
    HydrologyEvidencePage, HydrologyKind, HydrologyWaterPolicy, MAP_SCHEMA_VERSION, MapPackage,
    MapRequest, ModernLandCoverPage, PreparedEnvironment, PyramidLevel,
    WORLD_COVER_OBSERVATION_YEAR, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
};
use serde::Serialize;
use std::{
    fs,
    io::{ErrorKind, Write},
    path::Path,
};
mod chunks;
mod history;
mod water_model;

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

struct CheckedInSeed {
    target: &'static str,
    name: &'static str,
    bytes: &'static [u8],
}

const CHECKED_IN_SEEDS: [CheckedInSeed; 4] = [
    CheckedInSeed {
        target: "drs",
        name: "one-entry.drs",
        bytes: include_bytes!("../../../../fuzz/corpus/drs/one-entry.drs"),
    },
    CheckedInSeed {
        target: "manifest",
        name: "minimal.json",
        bytes: include_bytes!("../../../../fuzz/corpus/manifest/minimal.json"),
    },
    CheckedInSeed {
        target: "palette",
        name: "jasc.pal",
        bytes: include_bytes!("../../../../fuzz/corpus/palette/jasc.pal"),
    },
    CheckedInSeed {
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
    prepared_seeds.push(write(
        root,
        "environment_page",
        "hydrology-evidence",
        &serde_json::to_vec(&hydrology)?,
    )?);
    prepared_seeds.push(write(
        root,
        "environment_page",
        "modern-land-cover",
        &serde_json::to_vec(&land_cover)?,
    )?);
    prepared_seeds.extend(water_model::prepare(
        root,
        MapRequest::default(),
        &hydrology,
        &land_cover,
    )?);
    prepared_seeds.extend(history::prepare(root)?);
    prepared_seeds.extend(chunks::prepare(root)?);
    Ok(SeedInventory {
        prepared_seeds,
        verified_legacy_seeds: verify_legacy_seeds(root)?,
    })
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
    CHECKED_IN_SEEDS
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
mod tests;
