use aoe_map::{MapPackage, MapRequest};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::atomic::AtomicBool};

mod execution;
mod hydrology;
pub(crate) mod progress;
mod scratch;
use execution::execute;
pub(super) use scratch::recover as recover_scratch;

/// A vector-hydrology overview carries the landscape overview source locks
/// plus the pinned HydroLAKES and HydroRIVERS locks.
const VECTOR_OVERVIEW_SOURCE_LOCKS: usize =
    aoe_map::LANDSCAPE_OVERVIEW_SOURCE_LOCKS + aoe_map::VECTOR_HYDROLOGY_SOURCE_LOCKS;

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
    let mut input = serde_json::json!({
        "staging_root": scratch.root,
        "progress_path": progress_path,
        "operation": operation,
        "cache_root": cache_root,
        "output_directory": output_directory,
        "request": request,
        "samples_per_axis": preparation.samples_per_axis,
        "resolution": "glo30_prefer_glo90",
    });
    if let Some(axes) = preparation.field_axes {
        input["field_axes"] = serde_json::to_value(axes)
            .map_err(|error| format!("could not encode field axes: {error}"))?;
    }
    input["hydrology_mode"] =
        serde_json::to_value(preparation.hydrology_mode).map_err(|error| error.to_string())?;
    let input = serde_json::to_vec(&input)
        .map_err(|error| format!("could not encode map-worker request: {error}"))?;
    let output = execute(worker, input, cancelled, || monitor.poll())?;
    let package = decode_prepared_output(
        &output,
        request,
        preparation.samples_per_axis,
        preparation.field_axes,
    )?;
    if preparation.mode == crate::map_jobs::PreparationMode::Overview {
        hydrology::verify(&package, preparation.hydrology_mode)?;
    }
    Ok(package)
}

fn decode_prepared_output(
    output: &[u8],
    request: MapRequest,
    samples_per_axis: u16,
    field_axes: Option<crate::map_jobs::OverviewFieldAxes>,
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
    if let Some(axes) = field_axes
        && (package.environment.samples_per_axis != axes.elevation
            || package.environment.vegetation_samples_per_axis() != Some(axes.vegetation)
            || package.environment.water_samples_per_axis() != Some(axes.water)
            || package.environment.historical_samples_per_axis() != Some(axes.historical))
    {
        return Err("map worker returned different field axes than requested".to_owned());
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
#[path = "map_worker/tests/hydrology_response.rs"]
mod tests;
