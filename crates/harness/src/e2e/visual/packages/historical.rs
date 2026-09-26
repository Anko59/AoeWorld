use super::Result;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Clone, Debug, Default, Serialize)]
pub struct HistoricalLandUseEvidence {
    pub(super) level_zero_pages: usize,
    pub(super) coverage_present_pages: usize,
    pub(super) legacy_coverage_missing_pages: usize,
    pub(super) coverage_samples: usize,
    pub(super) land_percent_sum: u64,
    pub(super) valid_land_percent_sum: u64,
    pub(super) lake_percent_sum: u64,
    pub(super) ocean_percent_sum: u64,
    pub(super) nodata_percent_sum: u64,
    pub(super) outside_percent_sum: u64,
}

pub(super) fn read_historical_coverage(
    root: &Path,
    hash: &str,
) -> Result<HistoricalLandUseEvidence> {
    let directory = root.join("pages").join(hash).join("historical-land-use");
    let mut evidence = HistoricalLandUseEvidence::default();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let page: aoe_map::HistoricalLandUsePage = serde_json::from_slice(&bytes)?;
        if page.level != 0 {
            continue;
        }
        evidence.level_zero_pages += 1;
        if serde_json::from_slice::<serde_json::Value>(&bytes)?
            .get("coverage")
            .is_none()
        {
            evidence.legacy_coverage_missing_pages += 1;
            continue;
        }
        let expected_samples = usize::from(page.width) * usize::from(page.height);
        if page.coverage.len() != expected_samples {
            return Err(format!(
                "historical land-use coverage has {} samples, expected {expected_samples} in {}",
                page.coverage.len(),
                path.display()
            )
            .into());
        }
        evidence.coverage_present_pages += 1;
        for sample in &page.coverage {
            evidence.coverage_samples += 1;
            evidence.land_percent_sum +=
                required_percent(sample.land_percent, "land_percent", &path)?;
            evidence.valid_land_percent_sum +=
                required_percent(sample.valid_land_percent, "valid_land_percent", &path)?;
            evidence.lake_percent_sum +=
                required_percent(sample.lake_percent, "lake_percent", &path)?;
            evidence.ocean_percent_sum +=
                required_percent(sample.ocean_percent, "ocean_percent", &path)?;
            evidence.nodata_percent_sum +=
                required_percent(sample.nodata_percent, "nodata_percent", &path)?;
            evidence.outside_percent_sum +=
                required_percent(sample.outside_percent, "outside_percent", &path)?;
        }
    }
    if evidence.level_zero_pages == 0 {
        return Err(format!(
            "source-backed package {hash} has no level-zero historical land-use pages"
        )
        .into());
    }
    Ok(evidence)
}

fn required_percent(value: u8, field: &str, path: &Path) -> Result<u64> {
    (value <= 100).then_some(u64::from(value)).ok_or_else(|| {
        format!(
            "historical land-use coverage field {field} is invalid in {}",
            path.display()
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_known_coverage_and_legacy_unknown_pages_separately() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let directory = temporary.path().join("pages/hash/historical-land-use");
        fs::create_dir_all(&directory).expect("historical page directory");
        fs::write(
            directory.join("0-0-0.json"),
            r#"{"level":0,"x":0,"y":0,"width":2,"height":1,"crop_percent":[0,0],"grazing_percent":[0,0],"population_pressure_per_square_kilometer":[0,0],"coverage":[{"land_percent":100,"valid_land_percent":80,"lake_percent":0,"ocean_percent":0,"nodata_percent":0,"outside_percent":0},{"land_percent":40,"valid_land_percent":0,"lake_percent":20,"ocean_percent":10,"nodata_percent":20,"outside_percent":10}]}"#,
        )
        .expect("coverage page");
        fs::write(
            directory.join("0-1-0.json"),
            r#"{"level":0,"x":1,"y":0,"width":1,"height":1,"crop_percent":[0],"grazing_percent":[0],"population_pressure_per_square_kilometer":[0],"coverage":"646400000000"}"#,
        )
        .expect("compact coverage page");
        fs::write(
            directory.join("1-0-0.json"),
            r#"{"level":0,"x":0,"y":0,"width":1,"height":1,"crop_percent":[0],"grazing_percent":[0],"population_pressure_per_square_kilometer":[0]}"#,
        )
        .expect("legacy page");
        fs::write(
            directory.join("2-0-0.json"),
            r#"{"level":1,"x":0,"y":0,"width":1,"height":1,"crop_percent":[0],"grazing_percent":[0],"population_pressure_per_square_kilometer":[0]}"#,
        )
        .expect("nonzero level page");

        let evidence = read_historical_coverage(temporary.path(), "hash")
            .expect("read historical coverage evidence");

        assert_eq!(evidence.level_zero_pages, 3);
        assert_eq!(evidence.coverage_present_pages, 2);
        assert_eq!(evidence.legacy_coverage_missing_pages, 1);
        assert_eq!(evidence.coverage_samples, 3);
        assert_eq!(evidence.land_percent_sum, 240);
        assert_eq!(evidence.valid_land_percent_sum, 180);
        assert_eq!(evidence.lake_percent_sum, 20);
        assert_eq!(evidence.ocean_percent_sum, 10);
        assert_eq!(evidence.nodata_percent_sum, 20);
        assert_eq!(evidence.outside_percent_sum, 10);
    }
}
