use crate::{GeodataError, MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, local_aeqd_definition};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, FieldPyramid, HistoricalLandUsePage, MapRequest, PyramidLevel,
    ordered_land_use_page_root,
};
use gdal::{
    Dataset, GeoTransformEx,
    raster::ResampleAlg,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

mod area;
mod area_reader;
pub(super) mod area_stream;
mod correction;
pub use area::{
    HydeAreaAllocation, HydeAreaState, HydeGeographicPoint, HydeSourceAreaCell, HydeTargetAreaCell,
    allocate_hyde_area_window, prepare_hyde_area_pyramid,
};
pub use area_reader::prepare_hyde_area_600;
pub use correction::{
    HISTORICAL_CORRECTION_SCHEMA_VERSION, HISTORICAL_CORRECTION_TARGET_YEAR_CE,
    HistoricalCorrection, HistoricalCorrectionDocument, HistoricalCorrectionEvidence,
    HydeWholeCellQuantities, MAX_HISTORICAL_CORRECTION_JSON_BYTES,
    MAX_HISTORICAL_CORRECTION_SAMPLES_PER_AXIS,
};

const HYDE_600_MEMBERS: [&str; 5] = [
    "baseline/asc/600AD_lu/cropland600AD.asc",
    "baseline/asc/600AD_lu/grazing600AD.asc",
    "baseline/asc/600AD_pop/popc_600AD.asc",
    "general_files/landlake.asc",
    "general_files/maxln_cr.asc",
];
const MAX_HYDE_MEMBER_BYTES: u64 = 128 * 1024 * 1024;
/// Historical land-use fields may be prepared more finely than overview
/// elevation. This is an independent field limit, not an elevation limit.
pub const MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS: u16 = 1_024;

#[derive(Clone, Debug)]
pub struct PreparedHistoricalLandUse {
    pub field: FieldPyramid,
    pub pages: Vec<HistoricalLandUsePage>,
}

/// Samples HYDE's fixed 5' land/lake mask. The release notes define land as
/// one, lakes as zero, and ocean as nodata; only lakes become inland coverage.
pub fn prepare_hyde_lake_coverage(
    supplementary_archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<Vec<u8>, GeodataError> {
    area_reader::prepare_hyde_lake_coverage(supplementary_archive, request, samples_per_axis)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LandUseValue {
    crop_percent: u8,
    grazing_percent: u8,
    population_pressure_per_square_kilometer: u16,
}

/// Extracts only named 600 AD HYDE members after archive-path and size checks.
pub fn prepare_hyde_600(
    baseline_archive: &Path,
    supplementary_archive: &Path,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedHistoricalLandUse, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "working historical land-use grid is outside direct bounds",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid request"))?;
    let coordinates = projected_coordinates(request, samples_per_axis)?;
    let crop = sample_member(
        baseline_archive,
        HYDE_600_MEMBERS[0],
        &coordinates,
        samples_per_axis,
    )?;
    let grazing = sample_member(
        baseline_archive,
        HYDE_600_MEMBERS[1],
        &coordinates,
        samples_per_axis,
    )?;
    let population = sample_member(
        baseline_archive,
        HYDE_600_MEMBERS[2],
        &coordinates,
        samples_per_axis,
    )?;
    let land = sample_member(
        supplementary_archive,
        HYDE_600_MEMBERS[3],
        &coordinates,
        samples_per_axis,
    )?;
    let max_land_area = sample_member(
        supplementary_archive,
        HYDE_600_MEMBERS[4],
        &coordinates,
        samples_per_axis,
    )?;
    let values = crop
        .into_iter()
        .zip(grazing)
        .zip(population)
        .zip(land)
        .zip(max_land_area)
        .map(|((((crop, grazing), population), land), max_land_area)| {
            land_use_value(crop, grazing, population, land, max_land_area)
        })
        .collect::<Vec<_>>();
    pyramid(samples_per_axis, values)
}

fn projected_coordinates(
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<Vec<(f64, f64)>, GeodataError> {
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid request estimate"))?;
    let definition = local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    let mut target =
        SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
    let mut source = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&target, &source).map_err(|_| GeodataError::Projection)?;
    let side = estimate.effective_side_meters as f64;
    let spacing = side / f64::from(samples_per_axis);
    let mut east = Vec::with_capacity(usize::from(samples_per_axis).pow(2));
    let mut north = Vec::with_capacity(east.capacity());
    for y in 0..samples_per_axis {
        for x in 0..samples_per_axis {
            east.push(-side / 2.0 + (f64::from(x) + 0.5) * spacing);
            north.push(side / 2.0 - (f64::from(y) + 0.5) * spacing);
        }
    }
    transform
        .transform_coords(&mut east, &mut north, &mut [])
        .map_err(|_| GeodataError::Coordinate)?;
    Ok(east.into_iter().zip(north).collect())
}

fn sample_member(
    archive: &Path,
    member: &str,
    coordinates: &[(f64, f64)],
    samples_per_axis: u16,
) -> Result<Vec<Option<f64>>, GeodataError> {
    let path = extract_member(archive, member)?;
    let dataset = Dataset::open(path)?;
    let (width, height) = dataset.raster_size();
    let inverse = dataset.geo_transform()?.invert()?;
    let band = dataset.rasterband(1)?;
    let nodata = band.no_data_value();
    let values = coordinates
        .iter()
        .map(|&(longitude, latitude)| {
            let (pixel, line) = inverse.apply(longitude, latitude);
            let pixel = pixel.floor() as isize;
            let line = line.floor() as isize;
            if pixel < 0 || line < 0 || pixel >= width as isize || line >= height as isize {
                return Ok(None);
            }
            let value = band
                .read_as::<f64>(
                    (pixel, line),
                    (1, 1),
                    (1, 1),
                    Some(ResampleAlg::NearestNeighbour),
                )?
                .data()[0];
            Ok(
                (value.is_finite() && !nodata.is_some_and(|missing| value == missing))
                    .then_some(value),
            )
        })
        .collect::<Result<Vec<_>, gdal::errors::GdalError>>()?;
    (values.len() == usize::from(samples_per_axis).pow(2))
        .then_some(values)
        .ok_or(GeodataError::Preparation(
            "historical grid shape is invalid",
        ))
}

fn extract_member(archive: &Path, member: &str) -> Result<PathBuf, GeodataError> {
    if !HYDE_600_MEMBERS.contains(&member)
        || member.starts_with('/')
        || member
            .split('/')
            .any(|segment| segment == ".." || segment.is_empty())
    {
        return Err(GeodataError::Preparation(
            "historical archive member is invalid",
        ));
    }
    let archive = archive.canonicalize().map_err(GeodataError::Io)?;
    let archive_name =
        archive
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(GeodataError::Preparation(
                "historical archive path is invalid",
            ))?;
    let leaf = Path::new(member)
        .file_name()
        .ok_or(GeodataError::Preparation(
            "historical archive member is invalid",
        ))?;
    let directory = archive
        .parent()
        .ok_or(GeodataError::Preparation(
            "historical archive path is invalid",
        ))?
        .join("extracted")
        .join(archive_name);
    let destination = directory.join(leaf);
    if let Ok(metadata) = fs::symlink_metadata(&destination) {
        return (!metadata.file_type().is_symlink()
            && metadata.is_file()
            && metadata.len() <= MAX_HYDE_MEMBER_BYTES)
            .then_some(destination)
            .ok_or(GeodataError::Preparation(
                "historical extracted member is invalid",
            ));
    }
    let mut archive = ZipArchive::new(File::open(&archive)?)
        .map_err(|_| GeodataError::Preparation("historical ZIP archive is invalid"))?;
    let mut entry = archive
        .by_name(member)
        .map_err(|_| GeodataError::Preparation("historical ZIP member is missing"))?;
    if entry.enclosed_name() != Some(Path::new(member).to_path_buf())
        || entry.is_dir()
        || entry.size() > MAX_HYDE_MEMBER_BYTES
    {
        return Err(GeodataError::Preparation("historical ZIP member is unsafe"));
    }
    fs::create_dir_all(&directory)?;
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        leaf.to_string_lossy(),
        std::process::id()
    ));
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    let copied = io::copy(
        &mut entry.by_ref().take(MAX_HYDE_MEMBER_BYTES + 1),
        &mut output,
    )?;
    output.sync_all()?;
    if copied != entry.size() || copied > MAX_HYDE_MEMBER_BYTES {
        let _ = fs::remove_file(&temporary);
        return Err(GeodataError::Preparation(
            "historical ZIP member size is invalid",
        ));
    }
    match fs::hard_link(&temporary, &destination) {
        Ok(()) => fs::remove_file(temporary)?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => fs::remove_file(temporary)?,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(GeodataError::Io(error));
        }
    }
    Ok(destination)
}

fn land_use_value(
    crop: Option<f64>,
    grazing: Option<f64>,
    population: Option<f64>,
    land: Option<f64>,
    max_land_area: Option<f64>,
) -> LandUseValue {
    let (Some(land), Some(max_land_area)) = (land, max_land_area) else {
        return LandUseValue {
            crop_percent: 0,
            grazing_percent: 0,
            population_pressure_per_square_kilometer: 0,
        };
    };
    if land <= 0.0 || max_land_area <= 0.0 {
        return LandUseValue {
            crop_percent: 0,
            grazing_percent: 0,
            population_pressure_per_square_kilometer: 0,
        };
    }
    let crop_percent = fraction_percent(crop.unwrap_or(0.0), max_land_area);
    let grazing_percent =
        fraction_percent(grazing.unwrap_or(0.0), max_land_area).min(100 - crop_percent);
    LandUseValue {
        crop_percent,
        grazing_percent,
        population_pressure_per_square_kilometer: (population.unwrap_or(0.0) / max_land_area)
            .round()
            .clamp(0.0, f64::from(u16::MAX))
            as u16,
    }
}

fn fraction_percent(value: f64, land_area: f64) -> u8 {
    if value <= 0.0 || land_area <= 0.0 {
        return 0;
    }
    (value / land_area * 100.0).round().clamp(0.0, 100.0) as u8
}

fn pyramid(
    samples_per_axis: u16,
    values: Vec<LandUseValue>,
) -> Result<PreparedHistoricalLandUse, GeodataError> {
    let mut pages = Vec::new();
    let mut levels = Vec::new();
    let mut axis = samples_per_axis;
    let mut values = values;
    loop {
        let level_pages = pages_for(levels.len() as u8, axis, &values)?;
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_land_use_page_root(&level_pages)?,
        });
        pages.extend(level_pages);
        if axis == 1 {
            break;
        }
        values = reduce(axis, &values)?;
        axis = axis.div_ceil(2);
    }
    Ok(PreparedHistoricalLandUse {
        field: FieldPyramid { levels },
        pages,
    })
}

fn pages_for(
    level: u8,
    axis: u16,
    values: &[LandUseValue],
) -> Result<Vec<HistoricalLandUsePage>, GeodataError> {
    if values.len() != usize::from(axis).pow(2) {
        return Err(GeodataError::Preparation(
            "historical grid shape is invalid",
        ));
    }
    let mut pages = Vec::new();
    for y in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
        for x in (0..axis).step_by(usize::from(ENVIRONMENT_PAGE_SAMPLES)) {
            let width = (axis - x).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let height = (axis - y).min(u16::from(ENVIRONMENT_PAGE_SAMPLES));
            let mut crop_percent = Vec::with_capacity(usize::from(width) * usize::from(height));
            let mut grazing_percent = Vec::with_capacity(crop_percent.capacity());
            let mut population_pressure_per_square_kilometer =
                Vec::with_capacity(crop_percent.capacity());
            for row in y..y + height {
                for column in x..x + width {
                    let value = values[usize::from(row) * usize::from(axis) + usize::from(column)];
                    crop_percent.push(value.crop_percent);
                    grazing_percent.push(value.grazing_percent);
                    population_pressure_per_square_kilometer
                        .push(value.population_pressure_per_square_kilometer);
                }
            }
            pages.push(HistoricalLandUsePage {
                level,
                x: x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                y: y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
                width: width as u8,
                height: height as u8,
                crop_percent,
                grazing_percent,
                population_pressure_per_square_kilometer,
                coverage: Vec::new(),
            });
        }
    }
    Ok(pages)
}

fn reduce(axis: u16, values: &[LandUseValue]) -> Result<Vec<LandUseValue>, GeodataError> {
    if values.len() != usize::from(axis).pow(2) {
        return Err(GeodataError::Preparation(
            "historical grid shape is invalid",
        ));
    }
    let next_axis = axis.div_ceil(2);
    let mut reduced = Vec::with_capacity(usize::from(next_axis).pow(2));
    for y in 0..next_axis {
        for x in 0..next_axis {
            let mut crop = 0_u16;
            let mut grazing = 0_u16;
            let mut population = 0_u32;
            let mut count = 0_u16;
            for source_y in y * 2..((y + 1) * 2).min(axis) {
                for source_x in x * 2..((x + 1) * 2).min(axis) {
                    let value =
                        values[usize::from(source_y) * usize::from(axis) + usize::from(source_x)];
                    crop += u16::from(value.crop_percent);
                    grazing += u16::from(value.grazing_percent);
                    population += u32::from(value.population_pressure_per_square_kilometer);
                    count += 1;
                }
            }
            let crop_percent = (crop / count) as u8;
            let grazing_percent = ((grazing / count) as u8).min(100 - crop_percent);
            reduced.push(LandUseValue {
                crop_percent,
                grazing_percent,
                population_pressure_per_square_kilometer: (population / u32::from(count)) as u16,
            });
        }
    }
    Ok(reduced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn land_use_fractions_use_valid_land_and_never_double_count_grazing() {
        assert_eq!(
            land_use_value(Some(8.0), Some(9.0), Some(50.0), Some(1.0), Some(10.0)),
            LandUseValue {
                crop_percent: 80,
                grazing_percent: 20,
                population_pressure_per_square_kilometer: 5,
            }
        );
        assert_eq!(
            land_use_value(Some(8.0), Some(9.0), Some(50.0), Some(0.0), Some(10.0)),
            LandUseValue {
                crop_percent: 0,
                grazing_percent: 0,
                population_pressure_per_square_kilometer: 0,
            }
        );
    }

    #[test]
    fn archive_members_are_an_explicit_safe_allowlist() {
        assert!(HYDE_600_MEMBERS.contains(&"baseline/asc/600AD_lu/cropland600AD.asc"));
        assert!(!HYDE_600_MEMBERS.contains(&"../../outside.asc"));
    }
}

#[cfg(test)]
#[path = "tests/hyde.rs"]
mod hyde_tests;

#[cfg(test)]
#[path = "tests/hyde_area_reader.rs"]
mod area_reader_tests;
