//! Area-averaged Copernicus GLO-90 elevation for landscape overview maps.
use super::{Bounds, Tile};
use crate::{GeodataError, SourceCache, elevation::PreparedElevation};
use aoe_map::MapRequest;
use std::{path::Path, sync::atomic::AtomicBool};

pub(crate) const OVERVIEW_DEM_IDENTITY: &str = "copernicus-glo90-area-mean-gdal-0.19";
pub(crate) const OVERVIEW_ETOPO_FILL_IDENTITY: &str = "etopo-overview-gdal-0.19-glo90-fill";

pub(crate) fn plan_overview_tiles(_request: MapRequest) -> Result<Option<Bounds>, GeodataError> {
    Ok(None)
}

pub(crate) fn acquire_overview_tiles(
    _cache: &SourceCache,
    _bounds: Bounds,
    _cancelled: &AtomicBool,
) -> Result<Vec<Tile>, GeodataError> {
    Ok(Vec::new())
}

pub(crate) fn prepare_area_averaged_elevation(
    _coarse: &Path,
    _request: MapRequest,
    _samples_per_axis: u16,
    _tiles: &[Tile],
) -> Result<PreparedElevation, GeodataError> {
    Err(GeodataError::Preparation("not implemented"))
}
