use aoe_map::{MapPackage, MapRequest};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_ERROR_BYTES: usize = 8 * 1024;

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
) -> Result<MapPackage, String> {
    let request = request.normalized().map_err(|error| error.to_string())?;
    let operation = match preparation.mode {
        crate::map_jobs::PreparationMode::Detailed => "prepare_detailed_directory",
        crate::map_jobs::PreparationMode::Overview => "prepare_overview_directory",
        crate::map_jobs::PreparationMode::ProceduralFallback => {
            return Err("fallback is not a source-worker operation".to_owned());
        }
    };
    let input = serde_json::to_vec(&serde_json::json!({
        "operation": operation,
        "cache_root": cache_root,
        "output_directory": output_directory,
        "request": request,
        "samples_per_axis": preparation.samples_per_axis,
        "resolution": "glo30_prefer_glo90",
    }))
    .map_err(|error| format!("could not encode map-worker request: {error}"))?;
    let output = execute(worker, input, cancelled)?;
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
    let output = execute(worker, input, &cancelled)?;
    let WorkerOutput::GeographicFootprint { points, distortion } = serde_json::from_slice(&output)
        .map_err(|error| format!("invalid footprint response: {error}"))?
    else {
        return Err("map worker returned an unexpected footprint response".to_owned());
    };
    Ok(ProjectedFootprint { points, distortion })
}

fn execute(worker: &Path, input: Vec<u8>, cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err("map-worker request exceeds the configured bound".to_owned());
    }
    let mut child = Command::new(worker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start map worker: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "map worker did not expose standard input".to_owned())?
        .write_all(&input)
        .map_err(|error| format!("could not send map-worker request: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "map worker did not expose standard output".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "map worker did not expose standard error".to_owned())?;
    let output_reader = thread::spawn(move || read_limited(stdout, MAX_RESPONSE_BYTES + 1));
    let error_reader = thread::spawn(move || read_limited(stderr, MAX_ERROR_BYTES));
    let Some(status) = wait_for_worker(&mut child, cancelled)? else {
        let _ = output_reader.join();
        let _ = error_reader.join();
        return Err("map creation cancelled".to_owned());
    };
    let output = join_reader(output_reader, "response")?;
    let error = String::from_utf8(join_reader(error_reader, "error")?)
        .map_err(|error| format!("map-worker error was not UTF-8: {error}"))?;
    if output.len() > MAX_RESPONSE_BYTES {
        return Err("map-worker response exceeds the configured bound".to_owned());
    }
    if !status.success() {
        return Err(format!("map worker failed: {}", error.trim()));
    }
    Ok(output)
}

fn wait_for_worker(
    child: &mut std::process::Child,
    cancelled: &AtomicBool,
) -> Result<Option<std::process::ExitStatus>, String> {
    loop {
        if cancelled.load(Ordering::SeqCst) {
            if let Err(error) = child.kill()
                && error.kind() != std::io::ErrorKind::InvalidInput
            {
                return Err(format!("could not cancel map worker: {error}"));
            }
            child
                .wait()
                .map_err(|error| format!("could not wait for cancelled map worker: {error}"))?;
            return Ok(None);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not poll map worker: {error}"))?
        {
            return Ok(Some(status));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_limited(reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(limit);
    reader
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut output)
        .map_err(|error| error.to_string())?;
    Ok(output)
}

fn join_reader(
    reader: thread::JoinHandle<Result<Vec<u8>, String>>,
    stream: &str,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("map-worker {stream} reader panicked"))?
        .map_err(|error| format!("could not read map-worker {stream}: {error}"))
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

    #[test]
    fn output_reader_keeps_an_overflow_byte() {
        let output = read_limited(std::io::Cursor::new(*b"abcdef"), 3).expect("read output");
        assert_eq!(output, b"abcd");
    }
}
