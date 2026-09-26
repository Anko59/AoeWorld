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
        let page: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        if page.get("level").and_then(serde_json::Value::as_u64) != Some(0) {
            continue;
        }
        evidence.level_zero_pages += 1;
        let Some(coverage) = page.get("coverage") else {
            evidence.legacy_coverage_missing_pages += 1;
            continue;
        };
        let coverage = coverage.as_array().ok_or_else(|| {
            format!(
                "historical land-use coverage is not an array in {}",
                path.display()
            )
        })?;
        let expected_samples = page
            .get("width")
            .and_then(serde_json::Value::as_u64)
            .and_then(|width| {
                page.get("height")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|samples| usize::try_from(samples).ok())
            .ok_or_else(|| {
                format!(
                    "historical page dimensions are invalid in {}",
                    path.display()
                )
            })?;
        if coverage.len() != expected_samples {
            return Err(format!(
                "historical land-use coverage has {} samples, expected {expected_samples} in {}",
                coverage.len(),
                path.display()
            )
            .into());
        }
        evidence.coverage_present_pages += 1;
        for sample in coverage {
            evidence.coverage_samples += 1;
            evidence.land_percent_sum += required_percent(sample, "land_percent", &path)?;
            evidence.valid_land_percent_sum +=
                required_percent(sample, "valid_land_percent", &path)?;
            evidence.lake_percent_sum += required_percent(sample, "lake_percent", &path)?;
            evidence.ocean_percent_sum += required_percent(sample, "ocean_percent", &path)?;
            evidence.nodata_percent_sum += required_percent(sample, "nodata_percent", &path)?;
            evidence.outside_percent_sum += required_percent(sample, "outside_percent", &path)?;
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

fn required_percent(sample: &serde_json::Value, field: &str, path: &Path) -> Result<u64> {
    sample
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value <= 100)
        .ok_or_else(|| {
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
            r#"{"level":0,"width":2,"height":1,"coverage":[{"land_percent":100,"valid_land_percent":80,"lake_percent":0,"ocean_percent":0,"nodata_percent":0,"outside_percent":0},{"land_percent":40,"valid_land_percent":0,"lake_percent":20,"ocean_percent":10,"nodata_percent":20,"outside_percent":10}]}"#,
        )
        .expect("coverage page");
        fs::write(
            directory.join("1-0-0.json"),
            r#"{"level":0,"width":1,"height":1,"crop_percent":[0]}"#,
        )
        .expect("legacy page");
        fs::write(
            directory.join("2-0-0.json"),
            r#"{"level":1,"width":1,"height":1}"#,
        )
        .expect("nonzero level page");

        let evidence = read_historical_coverage(temporary.path(), "hash")
            .expect("read historical coverage evidence");

        assert_eq!(evidence.level_zero_pages, 2);
        assert_eq!(evidence.coverage_present_pages, 1);
        assert_eq!(evidence.legacy_coverage_missing_pages, 1);
        assert_eq!(evidence.coverage_samples, 2);
        assert_eq!(evidence.land_percent_sum, 140);
        assert_eq!(evidence.valid_land_percent_sum, 80);
        assert_eq!(evidence.lake_percent_sum, 20);
        assert_eq!(evidence.ocean_percent_sum, 10);
        assert_eq!(evidence.nodata_percent_sum, 20);
        assert_eq!(evidence.outside_percent_sum, 10);
    }
}
