use super::{
    GeneratedMap, GeodataError, WorkerRequest, WorkerResponse, etopo_2022_60s_surface,
    hyde_sources, local_aeqd_definition, potential_biome_sources, prepare_elevation,
    prepare_overview, project_wgs84, projected_footprint, projection_distortion, raster_dimensions,
    round_meters,
};

pub fn execute(request: WorkerRequest) -> Result<WorkerResponse, GeodataError> {
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
        } => {
            let prepared = prepare_overview(cache_root, request, samples_per_axis)?;
            let generated = GeneratedMap::from_prepared(request, prepared)?;
            generated.write_directory(&output_directory)?;
            Ok(WorkerResponse::PreparedDirectory {
                package: generated.package,
            })
        }
        WorkerRequest::PrepareDetailedDirectory {
            cache_root,
            output_directory,
            request,
            samples_per_axis,
            resolution,
            staging_root,
        } => Ok(WorkerResponse::PreparedDirectory {
            package: crate::copernicus::prepare_with_staging(
                cache_root,
                output_directory,
                request,
                samples_per_axis,
                resolution,
                staging_root,
            )?,
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
                environment: prepared.environment,
                pages: prepared.pages,
            })
        }
    }
}
