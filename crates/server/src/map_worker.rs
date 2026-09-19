use aoe_map::{
    ElevationPage, EnvironmentalProvenance, MAP_SCHEMA_VERSION, MapPackage, MapRequest,
    ProjectionMetadata, SourceLock, WaterPage,
};
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
    PreparedOverview(Box<PreparedOverview>),
    GeographicFootprint {
        points: Vec<GeographicPoint>,
        distortion: ProjectionDistortion,
    },
}

#[derive(Deserialize)]
struct PreparedOverview {
    source_lock: SourceLock,
    water_source_lock: SourceLock,
    vegetation_source_lock: SourceLock,
    vegetation_classes_source_lock: SourceLock,
    hyde_baseline_source_lock: SourceLock,
    hyde_supplementary_source_lock: SourceLock,
    hyde_readme_source_lock: SourceLock,
    projection: ProjectionMetadata,
    provenance: EnvironmentalProvenance,
    environment: aoe_map::PreparedEnvironment,
    pages: Vec<ElevationPage>,
    water_pages: Vec<WaterPage>,
    vegetation_pages: Vec<aoe_map::PotentialBiomePage>,
    historical_land_use_pages: Vec<aoe_map::HistoricalLandUsePage>,
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

pub(super) type PreparedOverviewPages = (
    MapPackage,
    Vec<ElevationPage>,
    Vec<WaterPage>,
    Vec<aoe_map::PotentialBiomePage>,
    Vec<aoe_map::HistoricalLandUsePage>,
);

pub(super) fn prepare_overview(
    worker: &Path,
    cache_root: &Path,
    request: MapRequest,
    cancelled: &AtomicBool,
) -> Result<PreparedOverviewPages, String> {
    let input = serde_json::to_vec(&serde_json::json!({
        "operation": "prepare_overview_elevation",
        "cache_root": cache_root,
        "request": request,
        "samples_per_axis": 128,
    }))
    .map_err(|error| format!("could not encode map-worker request: {error}"))?;
    let output = execute(worker, input, cancelled)?;
    let WorkerOutput::PreparedOverview(prepared) = serde_json::from_slice(&output)
        .map_err(|error| format!("invalid map-worker response: {error}"))?
    else {
        return Err("map worker returned an unexpected overview response".to_owned());
    };
    let PreparedOverview {
        source_lock,
        water_source_lock,
        vegetation_source_lock,
        vegetation_classes_source_lock,
        hyde_baseline_source_lock,
        hyde_supplementary_source_lock,
        hyde_readme_source_lock,
        projection,
        provenance,
        environment,
        pages,
        water_pages,
        vegetation_pages,
        historical_land_use_pages,
    } = *prepared;
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        request,
        vec![
            source_lock,
            water_source_lock,
            vegetation_source_lock,
            vegetation_classes_source_lock,
            hyde_baseline_source_lock,
            hyde_supplementary_source_lock,
            hyde_readme_source_lock,
        ],
        projection,
        provenance,
        environment,
    )
    .map_err(|error| error.to_string())?;
    Ok((
        package,
        pages,
        water_pages,
        vegetation_pages,
        historical_land_use_pages,
    ))
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
    fn output_reader_keeps_an_overflow_byte() {
        let output = read_limited(std::io::Cursor::new(*b"abcdef"), 3).expect("read output");
        assert_eq!(output, b"abcd");
    }
}
