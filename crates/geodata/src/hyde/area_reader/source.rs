use super::values::{
    RasterValue, classify_land_lake_value, classify_raster_value, quantity_value,
    validate_target_geography,
};
use super::{
    HYDE_600_MEMBERS, HydeAreaState, HydeGeographicPoint, HydeSourceAreaCell, HydeTargetAreaCell,
};
use crate::GeodataError;
use gdal::{Dataset, GeoTransformEx, raster::ResampleAlg};
use std::path::Path;

const MAX_SOURCE_WINDOW_CELLS: usize = 1_000_000;
const LONGITUDE_PERIOD_DEGREES: f64 = 360.0;

#[derive(Clone, Copy)]
pub(super) struct PixelWindow {
    pub(super) left: usize,
    pub(super) top: usize,
    pub(super) width: usize,
    pub(super) height: usize,
}

#[derive(Clone, Copy)]
pub(super) struct SourceWindow {
    pub(super) pixels: PixelWindow,
    pub(super) longitude_offset_degrees: f64,
}

pub(super) struct RasterSource {
    dataset: Dataset,
    width: usize,
    height: usize,
    transform: [f64; 6],
    inverse: [f64; 6],
    nodata: Option<f64>,
}

pub(super) struct ArchiveReader {
    crop: RasterSource,
    grazing: RasterSource,
    population: RasterSource,
    land_lake: RasterSource,
    valid_land: RasterSource,
}

impl RasterSource {
    pub(super) fn open(archive: &Path, member: &str) -> Result<Self, GeodataError> {
        let path = super::super::extract_member(archive, member)?;
        let dataset = Dataset::open(path)?;
        let (width, height) = dataset.raster_size();
        let transform = dataset.geo_transform()?;
        let inverse = transform.invert()?;
        let nodata = dataset.rasterband(1)?.no_data_value();
        Ok(Self {
            dataset,
            width,
            height,
            transform,
            inverse,
            nodata,
        })
    }

    pub(super) fn values(&self, window: PixelWindow) -> Result<Vec<RasterValue>, GeodataError> {
        let buffer = self.dataset.rasterband(1)?.read_as::<f64>(
            (window.left as isize, window.top as isize),
            (window.width, window.height),
            (window.width, window.height),
            Some(ResampleAlg::NearestNeighbour),
        )?;
        Ok(buffer
            .data()
            .iter()
            .map(|&value| classify_raster_value(value, self.nodata))
            .collect())
    }

    pub(super) fn cell_polygon(
        &self,
        column: usize,
        row: usize,
        longitude_offset_degrees: f64,
    ) -> Vec<HydeGeographicPoint> {
        let at = |column: usize, row: usize| HydeGeographicPoint {
            longitude_degrees: self.transform[0]
                + column as f64 * self.transform[1]
                + row as f64 * self.transform[2]
                + longitude_offset_degrees,
            latitude_degrees: self.transform[3]
                + column as f64 * self.transform[4]
                + row as f64 * self.transform[5],
        };
        vec![
            at(column, row),
            at(column + 1, row),
            at(column + 1, row + 1),
            at(column, row + 1),
        ]
    }

    pub(super) fn windows_for_targets(
        &self,
        targets: &[HydeTargetAreaCell],
    ) -> Result<Vec<SourceWindow>, GeodataError> {
        validate_target_geography(targets)?;
        let mut min_longitude = f64::INFINITY;
        let mut max_longitude = f64::NEG_INFINITY;
        for point in targets.iter().flat_map(|target| &target.polygon) {
            min_longitude = min_longitude.min(point.longitude_degrees);
            max_longitude = max_longitude.max(point.longitude_degrees);
        }
        if !min_longitude.is_finite() || !max_longitude.is_finite() {
            return Err(GeodataError::Preparation("HYDE target page is empty"));
        }

        let (first_turn, last_turn) = if self.has_global_longitude_period() {
            let (raster_west, raster_east) = self.longitude_extent();
            (
                ((min_longitude - raster_east) / LONGITUDE_PERIOD_DEGREES).ceil() as i32,
                ((max_longitude - raster_west) / LONGITUDE_PERIOD_DEGREES).floor() as i32,
            )
        } else {
            (0, 0)
        };
        if last_turn < first_turn {
            return Ok(Vec::new());
        }
        if last_turn - first_turn > 3 {
            return Err(GeodataError::Preparation(
                "HYDE target page exceeds the wrap-window limit",
            ));
        }

        let mut windows = Vec::new();
        let mut total_cells = 0_usize;
        for turn in first_turn..=last_turn {
            let longitude_offset = f64::from(turn) * LONGITUDE_PERIOD_DEGREES;
            let mut min_column = f64::INFINITY;
            let mut min_row = f64::INFINITY;
            let mut max_column = f64::NEG_INFINITY;
            let mut max_row = f64::NEG_INFINITY;
            for point in targets.iter().flat_map(|target| &target.polygon) {
                let (column, row) = self.inverse.apply(
                    point.longitude_degrees - longitude_offset,
                    point.latitude_degrees,
                );
                if !column.is_finite() || !row.is_finite() {
                    return Err(GeodataError::Coordinate);
                }
                min_column = min_column.min(column);
                min_row = min_row.min(row);
                max_column = max_column.max(column);
                max_row = max_row.max(row);
            }
            let left = min_column.floor().clamp(0.0, self.width as f64) as usize;
            let top = min_row.floor().clamp(0.0, self.height as f64) as usize;
            let right = max_column.ceil().clamp(0.0, self.width as f64) as usize;
            let bottom = max_row.ceil().clamp(0.0, self.height as f64) as usize;
            if right <= left || bottom <= top {
                continue;
            }
            let width = right - left;
            let height = bottom - top;
            let cells = width.saturating_mul(height);
            total_cells = total_cells.saturating_add(cells);
            if total_cells > MAX_SOURCE_WINDOW_CELLS {
                return Err(GeodataError::Preparation(
                    "HYDE archive windows exceed the source-cell limit",
                ));
            }
            windows.push(SourceWindow {
                pixels: PixelWindow {
                    left,
                    top,
                    width,
                    height,
                },
                longitude_offset_degrees: longitude_offset,
            });
        }
        Ok(windows)
    }

    fn has_global_longitude_period(&self) -> bool {
        self.transform[2].abs() <= 1.0e-12
            && self.transform[4].abs() <= 1.0e-12
            && ((self.width as f64 * self.transform[1].abs()) - LONGITUDE_PERIOD_DEGREES).abs()
                <= 1.0e-6
    }

    fn longitude_extent(&self) -> (f64, f64) {
        let opposite = self.transform[0] + self.width as f64 * self.transform[1];
        (
            self.transform[0].min(opposite),
            self.transform[0].max(opposite),
        )
    }
}

impl ArchiveReader {
    pub(super) fn open(baseline: &Path, supplementary: &Path) -> Result<Self, GeodataError> {
        let reader = Self {
            crop: RasterSource::open(baseline, HYDE_600_MEMBERS[0])?,
            grazing: RasterSource::open(baseline, HYDE_600_MEMBERS[1])?,
            population: RasterSource::open(baseline, HYDE_600_MEMBERS[2])?,
            land_lake: RasterSource::open(supplementary, HYDE_600_MEMBERS[3])?,
            valid_land: RasterSource::open(supplementary, HYDE_600_MEMBERS[4])?,
        };
        reader.validate_grids()?;
        Ok(reader)
    }

    fn validate_grids(&self) -> Result<(), GeodataError> {
        let reference = &self.land_lake;
        for source in [
            &self.crop,
            &self.grazing,
            &self.population,
            &self.valid_land,
        ] {
            if source.width != reference.width
                || source.height != reference.height
                || source
                    .transform
                    .iter()
                    .zip(reference.transform)
                    .any(|(left, right)| (left - right).abs() > right.abs().max(1.0) * 1.0e-12)
            {
                return Err(GeodataError::Preparation(
                    "HYDE source members use different grids",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn source_cells(
        &self,
        targets: &[HydeTargetAreaCell],
    ) -> Result<Vec<HydeSourceAreaCell>, GeodataError> {
        let windows = self.land_lake.windows_for_targets(targets)?;
        let mut cells = Vec::new();
        for source_window in windows {
            let window = source_window.pixels;
            let land_lake = self.land_lake.values(window)?;
            let crop = self.crop.values(window)?;
            let grazing = self.grazing.values(window)?;
            let population = self.population.values(window)?;
            let valid_land = self.valid_land.values(window)?;
            cells.reserve(land_lake.len());
            for row in 0..window.height {
                for column in 0..window.width {
                    let index = row * window.width + column;
                    let state = classify_land_lake_value(land_lake[index])?;
                    let (crop, grazing, population, valid_area) = if state == HydeAreaState::Land {
                        let (Some(crop), Some(grazing), Some(population), Some(valid_area)) = (
                            quantity_value(crop[index])?,
                            quantity_value(grazing[index])?,
                            quantity_value(population[index])?,
                            quantity_value(valid_land[index])?,
                        ) else {
                            return Err(GeodataError::Preparation(
                                "HYDE land cell is missing a required quantity",
                            ));
                        };
                        (
                            Some(crop),
                            Some(grazing),
                            Some(population),
                            Some(valid_area),
                        )
                    } else {
                        (None, None, None, None)
                    };
                    cells.push(HydeSourceAreaCell {
                        polygon: self.land_lake.cell_polygon(
                            window.left + column,
                            window.top + row,
                            source_window.longitude_offset_degrees,
                        ),
                        state,
                        crop_area_square_kilometers: crop,
                        grazing_area_square_kilometers: grazing,
                        population,
                        valid_land_area_square_kilometers: valid_area,
                    });
                }
            }
        }
        Ok(cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::{MapRequest, Ratio};
    use std::{
        fs::{self, File},
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn fixed_fiji_page_uses_two_bounded_source_windows_across_the_dateline() {
        let archive = global_mask_fixture();
        let request = MapRequest {
            center_latitude_e7: -178_000_000,
            center_longitude_e7: 1_798_000_000,
            requested_side_meters: 80_000,
            compression: Ratio::new(80, 1).expect("valid compression"),
            ..MapRequest::default()
        };
        let transform = super::super::target_to_wgs84(request).expect("projection transform");
        let side = request
            .estimate()
            .expect("valid Fiji footprint")
            .effective_side_meters;
        let (targets, _) = super::super::target_page(
            &transform,
            side,
            80,
            super::super::PageBounds {
                x: 0,
                y: 0,
                width: 64,
                height: 64,
            },
            179.8,
        )
        .expect("transformed Fiji target page");
        let longitudes = targets
            .iter()
            .flat_map(|target| &target.polygon)
            .map(|point| point.longitude_degrees)
            .collect::<Vec<_>>();
        assert!(longitudes.iter().any(|longitude| *longitude > 180.0));
        assert!(
            longitudes
                .iter()
                .all(|longitude| (179.0..181.0).contains(longitude))
        );

        let raster = RasterSource::open(&archive.path, HYDE_600_MEMBERS[3])
            .expect("open global mask fixture");
        let windows = raster
            .windows_for_targets(&targets)
            .expect("split Fiji target page");
        assert_eq!(windows.len(), 2);
        assert!(
            windows
                .iter()
                .map(|window| window.pixels.width)
                .sum::<usize>()
                < 10
        );
        assert!(
            windows
                .iter()
                .map(|window| window.pixels.width * window.pixels.height)
                .sum::<usize>()
                < 100
        );
        assert_ne!(
            windows[0].longitude_offset_degrees,
            windows[1].longitude_offset_degrees
        );
    }

    struct ArchiveFixture {
        root: std::path::PathBuf,
        path: std::path::PathBuf,
    }

    impl Drop for ArchiveFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn global_mask_fixture() -> ArchiveFixture {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aoe-hyde-global-mask-{}-{timestamp}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("fixture directory");
        let path = root.join("supplementary.zip");
        let mut output = ZipWriter::new(File::create(&path).expect("fixture archive"));
        output
            .start_file(HYDE_600_MEMBERS[3], SimpleFileOptions::default())
            .expect("mask member");
        let mut grid = String::from(
            "ncols 360\nnrows 180\nxllcorner -180\nyllcorner -90\ncellsize 1\nNODATA_value -9999\n",
        );
        let row = format!("{}\n", "1 ".repeat(360));
        for _ in 0..180 {
            grid.push_str(&row);
        }
        output.write_all(grid.as_bytes()).expect("mask grid");
        output.finish().expect("finish mask archive");
        ArchiveFixture { root, path }
    }
}
