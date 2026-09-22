use super::Result;
use aoe_map::{
    CompactChunk, ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyObservation,
    HydrologyWaterPolicy, MAP_SCHEMA_VERSION, MapChunkGenerator, MapPackage, MapRequest,
    ModernLandCoverPage, PreparedEnvironment, PyramidLevel, WORLD_COVER_OBSERVATION_YEAR,
    ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
};
use std::{fs, path::Path};

/// Valid seeds reach validation and roundtrip paths from the first smoke run.
/// Generated inputs and discovered mutations stay in the ignored corpus tree.
pub(super) fn prepare(root: &Path) -> Result<()> {
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())?;
    write(
        root,
        "map_package",
        "package",
        &serde_json::to_vec(&package)?,
    )?;
    write(
        root,
        "map_package",
        "request",
        &serde_json::to_vec(&package.request)?,
    )?;
    let hydrology = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::Lake as u8; 4],
        method: vec![HydrologyEvidenceMethod::HydroLakesExtent as u8; 4],
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
    write(
        root,
        "map_package",
        "typed-package",
        &serde_json::to_vec(&typed)?,
    )?;
    write(
        root,
        "map_package",
        "schema8-default",
        include_bytes!("../../../map/tests/fixtures/schema8-default-package.json"),
    )?;
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
        write(root, "environment_page", name, &serde_json::to_vec(&page)?)?;
    }
    write(
        root,
        "environment_page",
        "hydrology-evidence",
        &serde_json::to_vec(&hydrology)?,
    )?;
    write(
        root,
        "environment_page",
        "modern-land-cover",
        &serde_json::to_vec(&land_cover)?,
    )?;
    let chunk = MapChunkGenerator::new([3; 32], 7, 32).chunk(0, 0)?;
    write(root, "map_chunk", "full-chunk", &chunk_bytes(&chunk)?)?;
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
    write(root, "map_chunk", "typed-v2", &chunk_bytes(&observed)?)?;
    write(root, "map_chunk", "empty-legacy", &[1, 0, 0, 0, 0])?;
    write(
        root,
        "map_chunk",
        "one-tile-v1",
        &[
            1, 1, 0, 0, 0, 2, 0xde, 0xff, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9, 0xff,
            0xa9, 0xff, 0x65, 0xc3, 0x16, 0x00,
        ],
    )?;
    Ok(())
}

fn chunk_bytes(chunk: &aoe_map::Chunk) -> Result<Vec<u8>> {
    let hex = CompactChunk::encode(chunk)?.payload_hex;
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect()
}
fn write(root: &Path, target: &str, name: &str, bytes: &[u8]) -> Result<()> {
    let directory = root.join("fuzz/corpus").join(target);
    fs::create_dir_all(&directory)?;
    fs::write(directory.join(name), bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeds_reach_valid_parsers_and_preserve_discovered_inputs() {
        let root = tempfile::tempdir().unwrap();
        prepare(root.path()).unwrap();
        let corpus = root.path().join("fuzz/corpus");
        let package: MapPackage =
            serde_json::from_slice(&fs::read(corpus.join("map_package/package")).unwrap()).unwrap();
        package.validate().unwrap();
        for name in ["typed-package", "schema8-default"] {
            let package: MapPackage =
                serde_json::from_slice(&fs::read(corpus.join("map_package").join(name)).unwrap())
                    .unwrap();
            package.validate().unwrap();
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
        check!("modern-land-cover", aoe_map::ModernLandCoverPage);
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
}
