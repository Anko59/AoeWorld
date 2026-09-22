use aoe_map::{MapPackage, MapRequest};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::atomic::AtomicBool};

mod execution;
pub(crate) mod progress;
mod scratch;
use execution::execute;
pub(super) use scratch::recover as recover_scratch;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum WorkerOutput {
    PreparedDirectory {
        package: Box<MapPackage>,
    },
    GeographicFootprint {
        points: Vec<GeographicPoint>,
        distortion: ProjectionDistortion,
    },
}

#[derive(Deserialize, Serialize)]
pub(super) struct GeographicPoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

#[derive(Deserialize, Serialize)]
pub(super) struct ProjectionDistortion {
    pub min_scale_error_ppm: i64,
    pub max_scale_error_ppm: i64,
}

#[derive(Serialize)]
pub(super) struct ProjectedFootprint {
    pub points: Vec<GeographicPoint>,
    pub distortion: ProjectionDistortion,
}

pub(super) fn prepare(
    worker: &Path,
    cache_root: &Path,
    output_directory: &Path,
    request: MapRequest,
    preparation: crate::map_jobs::PreparationPlan,
    cancelled: &AtomicBool,
    progress_state: progress::State,
) -> Result<MapPackage, String> {
    let request = request.normalized().map_err(|error| error.to_string())?;
    let operation = match preparation.mode {
        crate::map_jobs::PreparationMode::Detailed => "prepare_detailed_directory",
        crate::map_jobs::PreparationMode::Overview => "prepare_overview_directory",
        crate::map_jobs::PreparationMode::ProceduralFallback => {
            return Err("fallback is not a source-worker operation".to_owned());
        }
    };
    let scratch = scratch::Scratch::new(cache_root)?;
    let progress_path = scratch.root.join("progress.json");
    let mut monitor = progress::Monitor::new(progress_path.clone(), progress_state);
    let input = serde_json::to_vec(&serde_json::json!({
        "staging_root": scratch.root,
        "progress_path": progress_path,
        "operation": operation,
        "cache_root": cache_root,
        "output_directory": output_directory,
        "request": request,
        "samples_per_axis": preparation.samples_per_axis,
        "resolution": "glo30_prefer_glo90",
    }))
    .map_err(|error| format!("could not encode map-worker request: {error}"))?;
    let output = execute(worker, input, cancelled, || monitor.poll())?;
    decode_prepared_output(&output, request, preparation.samples_per_axis)
}

fn decode_prepared_output(
    output: &[u8],
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<MapPackage, String> {
    let WorkerOutput::PreparedDirectory { package } = serde_json::from_slice(output)
        .map_err(|error| format!("invalid map-worker response: {error}"))?
    else {
        return Err("map worker returned an unexpected directory response".to_owned());
    };
    package.validate().map_err(|error| error.to_string())?;
    if package.request != request {
        return Err("map worker returned a package for a different request".to_owned());
    }
    if package.environment.samples_per_axis == 0 || package.source_locks.is_empty() {
        return Err("source-backed preparation returned no environmental sources".to_owned());
    }
    if package.environment.samples_per_axis != samples_per_axis {
        return Err("map worker returned a different preparation detail than requested".to_owned());
    }
    Ok(*package)
}

pub(super) fn geographic_footprint(
    worker: &Path,
    request: MapRequest,
) -> Result<ProjectedFootprint, String> {
    let input = serde_json::to_vec(&serde_json::json!({
        "operation": "project_footprint",
        "request": request,
        "samples_per_edge": 16,
    }))
    .map_err(|error| format!("could not encode footprint request: {error}"))?;
    let cancelled = AtomicBool::new(false);
    let output = execution::execute_with_deadline(
        worker,
        input,
        &cancelled,
        std::time::Duration::from_secs(30),
    )?;
    let WorkerOutput::GeographicFootprint { points, distortion } = serde_json::from_slice(&output)
        .map_err(|error| format!("invalid footprint response: {error}"))?
    else {
        return Err("map worker returned an unexpected footprint response".to_owned());
    };
    Ok(ProjectedFootprint { points, distortion })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_source_response_cannot_silently_downgrade_sample_detail() {
        let request = MapRequest::default();
        let source = aoe_map::SourceLock {
            id: "fixture".into(),
            provider: "fixture".into(),
            release: "v1".into(),
            url: "https://example.invalid/fixture".into(),
            sha256: [1; 32],
            acquired_at: "fixture".into(),
            native_resolution: "30 meters".into(),
            crs: "EPSG:4326".into(),
            vertical_datum: "EGM2008".into(),
            license: "fixture".into(),
            preprocessing_version: "v1".into(),
        };
        let environment = aoe_map::PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 15_000_000,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            elevation: aoe_map::FieldPyramid {
                levels: vec![
                    aoe_map::PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: [1; 32],
                    },
                    aoe_map::PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: [2; 32],
                    },
                ],
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
        };
        let package = MapPackage::with_prepared_environment(
            aoe_map::MAP_SCHEMA_VERSION,
            request,
            vec![source],
            aoe_map::ProjectionMetadata::default(),
            aoe_map::EnvironmentalProvenance::default(),
            environment,
        )
        .expect("source fixture");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "operation": "prepared_directory", "package": package,
        }))
        .expect("worker response");
        assert!(decode_prepared_output(&bytes, request, 2).is_ok());
        assert!(
            decode_prepared_output(&bytes, request, 1024)
                .expect_err("wrong detail")
                .contains("different preparation detail")
        );
    }

    #[test]
    fn directory_response_cannot_substitute_another_request_or_silent_fallback() {
        let request = MapRequest::default();
        let package =
            MapPackage::new(aoe_map::MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
        let output = serde_json::to_vec(&serde_json::json!({
            "operation": "prepared_directory", "package": package,
        }))
        .expect("response");
        let mut other = request;
        other.seed += 1;
        assert!(
            decode_prepared_output(&output, other, 128)
                .expect_err("different request")
                .contains("different request")
        );
        assert!(
            decode_prepared_output(&output, request, 128)
                .expect_err("fallback")
                .contains("no environmental sources")
        );
    }
}
