use super::{
    GeneratedMap, GeodataError, WorkerRequest, WorkerResponse, etopo_2022_60s_surface,
    hyde_sources, local_aeqd_definition, potential_biome_sources, prepare_elevation,
    prepare_overview, prepare_overview_with_field_axes, project_wgs84, projected_footprint,
    projection_distortion, raster_dimensions, round_meters,
};

mod overview_hydrology;
pub use overview_hydrology::{
    OverviewHydrologyPreflight, preflight as preflight_overview_hydrology,
};

pub fn execute(request: WorkerRequest) -> Result<WorkerResponse, GeodataError> {
    execute_with_cancellation(request, &std::sync::atomic::AtomicBool::new(false))
}

pub fn execute_with_cancellation(
    request: WorkerRequest,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<WorkerResponse, GeodataError> {
    execute_inner(request, cancelled, None)
}

pub fn execute_overview_with_preflight(
    request: WorkerRequest,
    plan: OverviewHydrologyPreflight,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<WorkerResponse, GeodataError> {
    if !matches!(&request, WorkerRequest::PrepareOverviewDirectory { .. }) {
        return Err(GeodataError::Preparation(
            "hydrology preflight is only for overview directory preparation",
        ));
    }
    execute_inner(request, cancelled, Some(plan))
}

fn execute_inner(
    request: WorkerRequest,
    cancelled: &std::sync::atomic::AtomicBool,
    preflight: Option<OverviewHydrologyPreflight>,
) -> Result<WorkerResponse, GeodataError> {
    if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(crate::CacheError::Cancelled.into());
    }
    match request {
        WorkerRequest::PrepareOverviewElevation {
            cache_root,
            request,
            samples_per_axis,
        } => Ok(WorkerResponse::PreparedOverview(Box::new(
            prepare_overview(cache_root, request, samples_per_axis)?,
        ))),
        WorkerRequest::PrepareOverviewDirectory {
            cache_root,
            output_directory,
            request,
            samples_per_axis,
            field_axes,
            hydrology_mode,
            water_corrections,
            historical_corrections,
            vegetation_corrections,
        } => {
            crate::overview::validate_field_axes(field_axes, true)?;
            if field_axes.elevation != samples_per_axis {
                return Err(GeodataError::Preparation(
                    "overview elevation axis does not match requested samples",
                ));
            }
            let plan = match preflight {
                Some(plan) => {
                    plan.validate_binding(
                        request,
                        hydrology_mode,
                        samples_per_axis,
                        field_axes,
                        &water_corrections,
                    )?;
                    plan
                }
                None => overview_hydrology::preflight(
                    hydrology_mode,
                    request,
                    samples_per_axis,
                    field_axes,
                    water_corrections,
                    cancelled,
                )?,
            };
            let prepared = prepare_overview_with_field_axes(
                cache_root.clone(),
                request,
                field_axes,
                historical_corrections.as_ref(),
                vegetation_corrections.as_ref(),
            )?;
            let hydrology = plan.prepare(
                cache_root,
                &prepared.pages,
                &prepared.water_pages,
                cancelled,
            )?;
            let generated = GeneratedMap::from_prepared(request, prepared)?;
            let generated = match hydrology {
                Some(hydrology) => generated.with_overview_vector_hydrology(hydrology)?,
                None => generated,
            };
            let mut generated = generated;
            generated.package = plan.mark_package(generated.package)?;
            crate::preparation_progress::stage(
                crate::preparation_progress::Phase::PublishingPackage,
            );
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(crate::CacheError::Cancelled.into());
            }
            generated.write_directory_with_cancellation(&output_directory, cancelled)?;
            Ok(WorkerResponse::PreparedDirectory {
                package: Box::new(generated.package),
            })
        }
        WorkerRequest::PrepareDetailedDirectory {
            cache_root,
            output_directory,
            request,
            samples_per_axis,
            resolution,
            staging_root,
            historical_corrections,
            water_corrections,
            vegetation_corrections,
        } => Ok(WorkerResponse::PreparedDirectory {
            package: Box::new(crate::copernicus::prepare_with_staging_and_corrections(
                cache_root,
                output_directory,
                request,
                samples_per_axis,
                resolution,
                staging_root,
                crate::copernicus::DetailedCorrections {
                    historical: historical_corrections.as_ref(),
                    water: water_corrections,
                    vegetation: vegetation_corrections.as_ref(),
                },
            )?),
        }),
        WorkerRequest::ListOverviewSources => Ok(WorkerResponse::KnownSources {
            sources: vec![etopo_2022_60s_surface()],
        }),
        WorkerRequest::ListPotentialBiomeSources => Ok(WorkerResponse::KnownSources {
            sources: potential_biome_sources()?,
        }),
        WorkerRequest::ListHydeSources => Ok(WorkerResponse::KnownSources {
            sources: hyde_sources()?,
        }),
        WorkerRequest::InspectRaster { path } => {
            let dimensions = raster_dimensions(&path)?;
            Ok(WorkerResponse::RasterDimensions {
                width: dimensions.width,
                height: dimensions.height,
            })
        }
        WorkerRequest::ProjectPoint {
            center_latitude_e7,
            center_longitude_e7,
            longitude,
            latitude,
        } => {
            let definition = local_aeqd_definition(center_latitude_e7, center_longitude_e7);
            let (east_meters, north_meters) = project_wgs84(&definition, longitude, latitude)?;
            Ok(WorkerResponse::ProjectedPoint {
                east_meters: round_meters(east_meters)?,
                north_meters: round_meters(north_meters)?,
            })
        }
        WorkerRequest::ProjectFootprint {
            request,
            samples_per_edge,
        } => Ok(WorkerResponse::GeographicFootprint {
            points: projected_footprint(request, samples_per_edge)?,
            distortion: projection_distortion(request)?,
        }),
        WorkerRequest::PrepareElevation {
            path,
            request,
            samples_per_axis,
        } => {
            let prepared = prepare_elevation(&path, request, samples_per_axis)?;
            Ok(WorkerResponse::PreparedElevation {
                environment: Box::new(prepared.environment),
                pages: prepared.pages,
            })
        }
    }
}
