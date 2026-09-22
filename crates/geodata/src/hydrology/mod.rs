use crate::{
    GeodataError, KnownSource, SourceCache, acquisition_marker, hydrology_vector_sources,
    worldcover_sources_for_bounds_cached, worldcover_tile_ids,
};
use aoe_map::MapRequest;
use gdal::Dataset;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[path = "sampling.rs"]
mod hydrology_sampling;
use hydrology_sampling::{Bounds, request_bounds, tile_latitude, tile_longitude};
#[path = "sampler.rs"]
mod sampler;
use sampler::Sampler;

pub const MAX_HYDROLOGY_SAMPLES_PER_AXIS: u16 = 1_024;
const PAGE: u16 = 64;
const MAX_PAGE_FEATURES: usize = 10_000;
const MAX_PAGE_GEOMETRY_BYTES: usize = 16 * 1024 * 1024;
const WORLD_COVER_NODATA: u8 = 0;
const WORLD_COVER_PERMANENT_WATER: u8 = 80;
const WORLD_COVER_WETLAND: u8 = 90;
const LAKES_MEMBER: &str = "HydroLAKES_polys_v10.gdb";
const RIVERS_MEMBER: &str = "HydroRIVERS_v10_eu_shp/HydroRIVERS_v10_eu.shp";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum HydrologyKind {
    Land = 0,
    Ocean = 1,
    Lake = 2,
    River = 3,
    Shallow = 4,
    Reservoir = 5,
    UnknownWater = 6,
    RegulatedLake = 7,
    NoEvidence = 8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HydrologyPage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub kind: Vec<u8>,
    pub surface_height_centimeters: Vec<i32>,
    pub surface_height_known: Vec<u8>,
    pub barrier_edges: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModernLandCoverPage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub worldcover_class: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparedHydrology {
    pub samples_per_axis: u16,
    pub source_year: u16,
    pub source_locks: Vec<aoe_map::SourceLock>,
    pub hydrology_pages: Vec<HydrologyPage>,
    pub modern_land_cover_pages: Vec<ModernLandCoverPage>,
}

impl PreparedHydrology {
    /// Returns a modern override for the existing gameplay water field. HYDE
    /// 600 remains authoritative for historical coverage: reservoirs,
    /// wetlands, and unclassified WorldCover water are evidence only until a
    /// separate reconstruction gives them a defensible historical meaning.
    pub fn modern_water_override_at(
        &self,
        target_axis: u16,
        x: u16,
        y: u16,
    ) -> Result<Option<(u8, u8)>, GeodataError> {
        if target_axis == 0 || x >= target_axis || y >= target_axis {
            return Err(GeodataError::Preparation(
                "hydrology water coordinate is outside the grid",
            ));
        }
        let source_x = ((((u32::from(x) * 2 + 1) * u32::from(self.samples_per_axis))
            / (u32::from(target_axis) * 2)) as u16)
            .min(self.samples_per_axis.saturating_sub(1));
        let source_y = ((((u32::from(y) * 2 + 1) * u32::from(self.samples_per_axis))
            / (u32::from(target_axis) * 2)) as u16)
            .min(self.samples_per_axis.saturating_sub(1));
        let page = self
            .hydrology_pages
            .iter()
            .find(|page| page.level == 0 && page.x == source_x / PAGE && page.y == source_y / PAGE)
            .ok_or(GeodataError::Preparation("hydrology page is missing"))?;
        let expected_width = (self.samples_per_axis - page.x * PAGE).min(PAGE) as u8;
        let expected_height = (self.samples_per_axis - page.y * PAGE).min(PAGE) as u8;
        if page.width != expected_width
            || page.height != expected_height
            || page.kind.len() != usize::from(page.width) * usize::from(page.height)
        {
            return Err(GeodataError::Preparation("hydrology page shape is invalid"));
        }
        let index =
            usize::from(source_y % PAGE) * usize::from(page.width) + usize::from(source_x % PAGE);
        let kind = match page.kind.get(index).copied() {
            Some(value) => match value {
                0 => HydrologyKind::Land,
                1 => HydrologyKind::Ocean,
                2 => HydrologyKind::Lake,
                3 => HydrologyKind::River,
                4 => HydrologyKind::Shallow,
                5 => HydrologyKind::Reservoir,
                6 => HydrologyKind::UnknownWater,
                7 => HydrologyKind::RegulatedLake,
                8 => HydrologyKind::NoEvidence,
                _ => {
                    return Err(GeodataError::Preparation(
                        "hydrology page has an invalid kind",
                    ));
                }
            },
            None => return Err(GeodataError::Preparation("hydrology page value is missing")),
        };
        Ok(match kind {
            HydrologyKind::Ocean => Some((100, 0)),
            HydrologyKind::Lake | HydrologyKind::River => Some((0, 100)),
            HydrologyKind::Land
            | HydrologyKind::Shallow
            | HydrologyKind::Reservoir
            | HydrologyKind::UnknownWater
            | HydrologyKind::RegulatedLake
            | HydrologyKind::NoEvidence => None,
        })
    }
}

struct Tile {
    latitude: i32,
    longitude: i32,
    path: PathBuf,
}

struct OpenTile {
    latitude: i32,
    longitude: i32,
    dataset: Dataset,
}

pub(crate) struct HydrologySourcePlan {
    worldcover: Vec<KnownSource>,
    sources: Vec<KnownSource>,
    rivers_available: bool,
}

/// Acquires and samples the bounded modern-water evidence stack. Sources are
/// filtered by page through GDAL's spatial index; no regional geometry set is
/// retained between pages. HYDE and potential vegetation remain untouched.
pub fn prepare_hydrology(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedHydrology, GeodataError> {
    if !(2..=MAX_HYDROLOGY_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "hydrology preparation supports 2 through 1024 samples per axis",
        ));
    }
    let plan = preflight_hydrology(&cache_root, request)?;
    let overview = crate::prepare_overview(cache_root.clone(), request, 128)?;
    prepare_hydrology_with_plan(
        cache_root,
        request,
        samples_per_axis,
        plan,
        &overview.water_pages,
    )
}

pub(crate) fn preflight_hydrology(
    cache_root: &Path,
    request: MapRequest,
) -> Result<HydrologySourcePlan, GeodataError> {
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let bounds = request_bounds(request)?;
    worldcover_tile_ids(
        bounds.min_latitude,
        bounds.max_latitude,
        bounds.min_longitude,
        bounds.max_longitude,
    )?;
    let cache = SourceCache::new(
        cache_root.to_path_buf(),
        crate::DownloadPolicy {
            cache_quota_bytes: crate::DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: crate::MAX_HYDROLOGY_DOWNLOAD_BYTES,
        },
    )?;
    let worldcover = worldcover_sources_for_bounds_cached(
        bounds.min_latitude,
        bounds.max_latitude,
        bounds.min_longitude,
        bounds.max_longitude,
        &cache,
    )?;
    let rivers_available = supports_hydrorivers(bounds);
    let mut sources = worldcover.clone();
    sources.extend(
        hydrology_vector_sources()
            .into_iter()
            .filter(|source| source.id != "hydrorivers-v1.0-eu-shp" || rivers_available),
    );
    let estimate = cache.estimate_known_acquisition(&sources)?;
    if estimate.download_bytes > crate::MAX_HYDROLOGY_DOWNLOAD_BYTES {
        return Err(GeodataError::Preparation(
            "hydrology source batch exceeds the 2 GiB per-job transfer limit",
        ));
    }
    Ok(HydrologySourcePlan {
        worldcover,
        sources,
        rivers_available,
    })
}

fn supports_hydrorivers(bounds: Bounds) -> bool {
    bounds.min_latitude >= 36.0
        && bounds.max_latitude <= 60.0
        && bounds.min_longitude >= -12.0
        && bounds.max_longitude <= 25.0
}

pub(crate) fn prepare_hydrology_with_plan(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    plan: HydrologySourcePlan,
    overview_water_pages: &[aoe_map::WaterPage],
) -> Result<PreparedHydrology, GeodataError> {
    if !(2..=MAX_HYDROLOGY_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "hydrology preparation supports 2 through 1024 samples per axis",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let cache = SourceCache::new(
        cache_root,
        crate::DownloadPolicy {
            cache_quota_bytes: crate::DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: crate::MAX_HYDROLOGY_DOWNLOAD_BYTES,
        },
    )?;
    let estimate = cache.estimate_known_acquisition(&plan.sources)?;
    if estimate.download_bytes > crate::MAX_HYDROLOGY_DOWNLOAD_BYTES {
        return Err(GeodataError::Preparation(
            "hydrology source batch exceeds the 2 GiB per-job transfer limit",
        ));
    }
    let cancelled = AtomicBool::new(false);
    let mut locks = Vec::with_capacity(plan.sources.len());
    let mut paths = BTreeMap::new();
    for source in &plan.sources {
        if cancelled.load(Ordering::SeqCst) {
            return Err(crate::CacheError::Cancelled.into());
        }
        let lock = cache.acquire_known(source, &cancelled)?;
        let path = cache.object_path(&lock)?;
        paths.insert(source.id.clone(), path);
        locks.push(lock);
    }
    let worldcover_tiles = plan
        .worldcover
        .iter()
        .map(|source| {
            Ok(Tile {
                latitude: tile_latitude(&source.id)?,
                longitude: tile_longitude(&source.id)?,
                path: paths
                    .get(&source.id)
                    .cloned()
                    .ok_or(GeodataError::Preparation(
                        "WorldCover cache path is missing",
                    ))?,
            })
        })
        .collect::<Result<Vec<_>, GeodataError>>()?;
    let lakes = paths
        .get("hydrolakes-v1.0-global-gdb")
        .ok_or(GeodataError::Preparation(
            "HydroLAKES cache path is missing",
        ))?;
    let rivers = paths.get("hydrorivers-v1.0-eu-shp").map(PathBuf::as_path);
    let ocean =
        hydrology_sampling::resample_ocean_coverage(128, samples_per_axis, overview_water_pages)?;
    let mut sampler = Sampler::new(
        request,
        samples_per_axis,
        worldcover_tiles,
        ocean,
        lakes,
        rivers,
    )?;
    let (hydrology_pages, modern_land_cover_pages) = sampler.pages()?;
    let river_coverage = if plan.rivers_available {
        "hydrorivers=western-europe-covered"
    } else {
        "hydrorivers=unavailable-outside-western-europe"
    };
    let preprocessing = format!("hydrology-gdal-page-v2;{river_coverage}");
    let source_locks = locks
        .iter()
        .map(|lock| lock.to_map_source_lock(acquisition_marker(), preprocessing.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedHydrology {
        samples_per_axis,
        source_year: 2021,
        source_locks,
        hydrology_pages,
        modern_land_cover_pages,
    })
}

#[cfg(test)]
#[path = "tests/hydrology.rs"]
mod tests;
