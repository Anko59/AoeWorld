//! Local reproduction without starting a server. Exact-input gate snapshots
//! omit local assets, so this helper does nothing when packages are absent.
use super::*;

#[test]
fn dump_stored_minimap_rasters_when_packages_are_available() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("local-assets/map-packages");
    if !directory.exists() {
        return;
    }
    let output = root.join(".cache/minimap");
    std::fs::create_dir_all(&output).expect("minimap output directory");
    for (hash, package) in crate::map_store::load(Some(&directory)).expect("stored packages") {
        let provider = (package.environment.samples_per_axis != 0).then(|| {
            crate::map_store::PageResidency::open(&directory, &package, &|| false)
                .expect("verified page residency")
        });
        let start = std::time::Instant::now();
        let raster = package_minimap(&package, provider.clone(), &|| false).expect("server raster");
        assert!(start.elapsed() < super::super::super::minimap::MINIMAP_TIME_BUDGET);
        std::fs::write(output.join(format!("current-{hash}.awmm")), &raster).expect("write raster");
        // Export the same height samples for relief-only inspection and cell
        // spacing experiments; no terrain chunks are generated.
        let generator = if let Some(provider) = provider {
            package
                .generator_with_page_provider(provider)
                .expect("generator")
        } else {
            package.generator()
        };
        let side = minimap_samples(package.estimate.tiles_per_side);
        let mut heights = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let tile = TileCoord::new(
                    minimap_tile(package.estimate.tiles_per_side, side, x).expect("x"),
                    minimap_tile(package.estimate.tiles_per_side, side, y).expect("y"),
                );
                let point = generator
                    .landscape_point_with_cancel(tile, &|| false)
                    .expect("height query")
                    .expect("inside map");
                heights.extend_from_slice(
                    &point
                        .tile
                        .terrain
                        .geographic_height_centimeters
                        .to_le_bytes(),
                );
            }
        }
        std::fs::write(output.join(format!("current-{hash}.heights")), heights)
            .expect("write heights");
    }
}
