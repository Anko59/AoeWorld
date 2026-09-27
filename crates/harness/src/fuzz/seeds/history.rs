use super::{Result, Seed, write};
use aoe_map::{HistoricalCoverage, HistoricalLandUsePage};
use std::path::Path;

pub(super) fn prepare(root: &Path) -> Result<Vec<Seed>> {
    let page = HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 1,
        crop_percent: vec![20, 0],
        grazing_percent: vec![30, 0],
        population_pressure_per_square_kilometer: vec![10, 0],
        coverage: vec![
            HistoricalCoverage {
                land_percent: 80,
                valid_land_percent: 70,
                lake_percent: 20,
                ..HistoricalCoverage::default()
            },
            HistoricalCoverage {
                land_percent: 100,
                valid_land_percent: 100,
                ..HistoricalCoverage::default()
            },
        ],
    };
    page.validate()?;
    let compact_bytes = serde_json::to_vec(&page)?;
    let mut legacy = serde_json::to_value(&page)?;
    legacy["coverage"] = serde_json::to_value(&page.coverage)?;
    let legacy_bytes = serde_json::to_vec(&legacy)?;
    Ok(vec![
        write(
            root,
            "environment_page",
            "history-compact-coverage",
            &compact_bytes,
        )?,
        write(
            root,
            "environment_page",
            "history-legacy-coverage",
            &legacy_bytes,
        )?,
    ])
}
