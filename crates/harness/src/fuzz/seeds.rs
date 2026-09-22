use super::Result;
use aoe_map::{CompactChunk, MAP_SCHEMA_VERSION, MapChunkGenerator, MapPackage, MapRequest};
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
    let chunk = MapChunkGenerator::new([3; 32], 7, 32).chunk(0, 0)?;
    let hex = CompactChunk::encode(&chunk)?.payload_hex;
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect::<Result<Vec<u8>>>()?;
    write(root, "map_chunk", "full-chunk", &bytes)?;
    write(root, "map_chunk", "empty-legacy", &[1, 0, 0, 0, 0])?;
    Ok(())
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
        fs::write(corpus.join("map_chunk/discovered"), b"keep").unwrap();
        prepare(root.path()).unwrap();
        assert_eq!(
            fs::read(corpus.join("map_chunk/discovered")).unwrap(),
            b"keep"
        );
    }
}
