//! Synthetic prepared forest for browser play, not real-source qualification.
//! The one-sample complete pyramids describe constant fields over 512 tiles.
//! Existing terrain adapters call prepared samples `SourceDerived`; for this
//! fixture that means synthetic page input, NOT observed real-world geodata.
//! Empty locks, Fallback layer metadata and the explicit projection label retain
//! that distinction without changing production provenance or qualification.
use aoe_map::{
    ElevationPage, EnvironmentalProvenance, FieldPyramid, LayerProvenance, MapPackage, MapRequest,
    PotentialBiomePage, PreparedEnvironment, ProjectionMetadata, PyramidLevel, WaterPage,
    ordered_biome_page_root, ordered_page_root, ordered_water_page_root,
};
use std::{fs, io::Write, path::Path};

#[cfg(test)]
mod tests;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) struct Fixture {
    // TempDir owns only a newly created ignored directory, never the input root.
    pub(super) directory: tempfile::TempDir,
    pub(super) hash: String,
}

struct Fields {
    package: MapPackage,
    elevation: ElevationPage,
    water: WaterPage,
    vegetation: PotentialBiomePage,
}

fn fields() -> Result<Fields> {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![0],
    };
    let vegetation = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        // PNV class 8 maps to Biome::Temperate; values are synthetic.
        potential_biome_class: vec![8],
    };
    let pyramid = |root| FieldPyramid {
        levels: vec![PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: root,
        }],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 30_720_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: pyramid(ordered_page_root(std::slice::from_ref(&elevation))?),
        water: Some(pyramid(ordered_water_page_root(std::slice::from_ref(
            &water,
        ))?)),
        vegetation: Some(pyramid(ordered_biome_page_root(std::slice::from_ref(
            &vegetation,
        ))?)),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        aoe_map::MAP_SCHEMA_VERSION,
        MapRequest {
            center_latitude_e7: 0,
            center_longitude_e7: 0,
            requested_side_meters: 30_720,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata {
            horizontal_crs: "synthetic-test-only-constant-grid-v1".to_owned(),
            tool_version: "aoe-harness-e2e-synthetic-flat-temperate-v1".to_owned(),
            ..ProjectionMetadata::default()
        },
        EnvironmentalProvenance {
            elevation: LayerProvenance::Fallback,
            water: LayerProvenance::Fallback,
            vegetation: LayerProvenance::Fallback,
            historical_land_use: LayerProvenance::Fallback,
        },
        environment,
    )?;
    Ok(Fields {
        package,
        elevation,
        water,
        vegetation,
    })
}

/// Include existing packages without publishing anything into their private root.
/// MapStore verifies the copied manifests/pages before the disposable server starts.
pub(super) fn prepare(root: &Path, existing: Option<&Path>) -> Result<Fixture> {
    let target = root.join("target");
    fs::create_dir_all(&target)?;
    let directory = tempfile::Builder::new()
        .prefix("e2e-landscape-")
        .tempdir_in(target)?;
    if let Some(existing) = existing {
        include_existing(existing, directory.path())?;
    }
    let fields = fields()?;
    let hash = fields.package.content_hash_hex();
    let page_root = directory.path().join("pages").join(&hash);
    for (layer, bytes) in [
        ("elevation", serde_json::to_vec(&fields.elevation)?),
        ("water", serde_json::to_vec(&fields.water)?),
        ("vegetation", serde_json::to_vec(&fields.vegetation)?),
    ] {
        let layer_root = page_root.join(layer);
        fs::create_dir_all(&layer_root)?;
        publish(&layer_root.join("0-0-0.json"), &bytes)?;
    }
    publish(
        &directory.path().join(format!("{hash}.json")),
        &serde_json::to_vec(&fields.package)?,
    )?;
    verify_store(directory.path())?;
    Ok(Fixture { directory, hash })
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.try_exists()? {
        if fs::read(path)? != bytes {
            return Err("synthetic fixture collides with existing package bytes".into());
        }
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

fn config(directory: &Path) -> Result<aoe_server::Config> {
    Ok(aoe_server::Config {
        bind: "127.0.0.1:0".parse()?,
        scenario: aoe_scenario::named("smoke").ok_or("missing smoke scenario")?,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.to_owned()),
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    })
}

fn verify_store(directory: &Path) -> Result<()> {
    aoe_server::AppState::new(&config(directory)?, "synthetic-e2e-fixture-validation")?;
    Ok(())
}

fn include_existing(source: &Path, target: &Path) -> Result<()> {
    if !source.try_exists()? {
        return Ok(());
    }
    if !fs::symlink_metadata(source)?.is_dir() {
        return Err("existing package root must be a regular directory".into());
    }
    // Read-only production validation preserves the existing package root's
    // qualification semantics, including rejection of symlinked page ancestors.
    verify_store(source)?;
    let mut budget = CopyBudget::default();
    let mut count = 0;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        count += 1;
        if count >= 256 || !entry.file_type()?.is_file() || entry.metadata()?.len() > 65_536 {
            return Err("existing map manifests exceed isolated E2E staging bounds".into());
        }
        let package: MapPackage = serde_json::from_slice(&fs::read(&path)?)?;
        package.validate()?;
        let hash = package.content_hash_hex();
        if entry.file_name() != std::ffi::OsString::from(format!("{hash}.json")) {
            return Err("existing map manifest filename is not canonical".into());
        }
        budget.copy_file(&path, &target.join(entry.file_name()))?;
        let pages = source.join("pages").join(&hash);
        if pages.try_exists()? {
            budget.copy_tree(&pages, &target.join("pages").join(hash), 0)?;
        }
    }
    // Production validation catches incomplete pyramids, bad roots, symlinks,
    // and incompatible identities in the snapshot, without touching the source.
    verify_store(target)
}

#[derive(Default)]
struct CopyBudget {
    files: usize,
    bytes: u64,
}

impl CopyBudget {
    fn copy_file(&mut self, source: &Path, target: &Path) -> Result<()> {
        let metadata = fs::symlink_metadata(source)?;
        self.files += 1;
        self.bytes = self.bytes.saturating_add(metadata.len());
        if !metadata.is_file() || self.files > 100_000 || self.bytes > 1_073_741_824 {
            return Err("existing map snapshot exceeds bounded E2E copy limits".into());
        }
        fs::copy(source, target)?;
        Ok(())
    }

    fn copy_tree(&mut self, source: &Path, target: &Path, depth: usize) -> Result<()> {
        if depth > 2 || !fs::symlink_metadata(source)?.is_dir() {
            return Err("existing package pages contain an unsupported directory".into());
        }
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let destination = target.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                self.copy_tree(&entry.path(), &destination, depth + 1)?;
            } else {
                self.copy_file(&entry.path(), &destination)?;
            }
        }
        Ok(())
    }
}
