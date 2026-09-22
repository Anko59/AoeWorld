use super::{GeodataError, PAGE, Sampler, Stage};
use aoe_map::{
    ElevationPage, FieldPyramid, HistoricalLandUsePage, PageLayer, PageRootBuilder,
    PotentialBiomePage, PyramidLevel, WaterPage,
};
use std::collections::BTreeMap;

#[path = "pyramid/water.rs"]
mod water;
use water::coarse_water;
#[path = "pyramid/evidence.rs"]
mod evidence;

pub(super) fn store_hydrology_evidence(
    stage: &Stage,
    prepared: &crate::PreparedHydrology,
) -> Result<(), GeodataError> {
    evidence::store_hydrology_evidence(stage, prepared)
}

pub(super) struct DetailedFields {
    pub(super) elevation: FieldPyramid,
    pub(super) water: FieldPyramid,
    pub(super) vegetation: FieldPyramid,
    pub(super) historical_land_use: FieldPyramid,
}

pub(super) trait PageOps: Clone + serde::de::DeserializeOwned + serde::Serialize {
    type Value: Copy;
    fn layer() -> PageLayer;
    fn from_values(
        level: u8,
        x: u16,
        y: u16,
        width: u8,
        height: u8,
        values: Vec<Self::Value>,
    ) -> Self;
    fn value(&self, index: usize) -> Self::Value;
    fn width(&self) -> u8;
    fn height(&self) -> u8;
    fn hash(&self) -> Result<[u8; 32], GeodataError>;
}

impl PageOps for ElevationPage {
    type Value = i32;
    fn layer() -> PageLayer {
        PageLayer::Elevation
    }
    fn from_values(
        level: u8,
        x: u16,
        y: u16,
        width: u8,
        height: u8,
        values: Vec<Self::Value>,
    ) -> Self {
        Self {
            level,
            x,
            y,
            width,
            height,
            geographic_height_centimeters: values,
        }
    }
    fn value(&self, index: usize) -> Self::Value {
        self.geographic_height_centimeters[index]
    }
    fn width(&self) -> u8 {
        self.width
    }
    fn height(&self) -> u8 {
        self.height
    }
    fn hash(&self) -> Result<[u8; 32], GeodataError> {
        Ok(self.content_hash()?)
    }
}

impl PageOps for WaterPage {
    type Value = (u8, u8);
    fn layer() -> PageLayer {
        PageLayer::Water
    }
    fn from_values(
        level: u8,
        x: u16,
        y: u16,
        width: u8,
        height: u8,
        values: Vec<Self::Value>,
    ) -> Self {
        let (ocean, inland): (Vec<_>, Vec<_>) = values.into_iter().unzip();
        Self {
            level,
            x,
            y,
            width,
            height,
            ocean_coverage_percent: ocean,
            inland_coverage_percent: inland,
        }
    }
    fn value(&self, index: usize) -> Self::Value {
        (
            self.ocean_coverage_percent[index],
            self.inland_coverage_percent[index],
        )
    }
    fn width(&self) -> u8 {
        self.width
    }
    fn height(&self) -> u8 {
        self.height
    }
    fn hash(&self) -> Result<[u8; 32], GeodataError> {
        Ok(self.content_hash()?)
    }
}

impl PageOps for PotentialBiomePage {
    type Value = u8;
    fn layer() -> PageLayer {
        PageLayer::Vegetation
    }
    fn from_values(
        level: u8,
        x: u16,
        y: u16,
        width: u8,
        height: u8,
        values: Vec<Self::Value>,
    ) -> Self {
        Self {
            level,
            x,
            y,
            width,
            height,
            potential_biome_class: values,
        }
    }
    fn value(&self, index: usize) -> Self::Value {
        self.potential_biome_class[index]
    }
    fn width(&self) -> u8 {
        self.width
    }
    fn height(&self) -> u8 {
        self.height
    }
    fn hash(&self) -> Result<[u8; 32], GeodataError> {
        Ok(self.content_hash()?)
    }
}

impl PageOps for HistoricalLandUsePage {
    type Value = (u8, u8, u16);
    fn layer() -> PageLayer {
        PageLayer::HistoricalLandUse
    }
    fn from_values(
        level: u8,
        x: u16,
        y: u16,
        width: u8,
        height: u8,
        values: Vec<Self::Value>,
    ) -> Self {
        let mut crop = Vec::with_capacity(values.len());
        let mut grazing = Vec::with_capacity(values.len());
        let mut population = Vec::with_capacity(values.len());
        for (c, g, p) in values {
            crop.push(c);
            grazing.push(g);
            population.push(p);
        }
        Self {
            level,
            x,
            y,
            width,
            height,
            crop_percent: crop,
            grazing_percent: grazing,
            population_pressure_per_square_kilometer: population,
        }
    }
    fn value(&self, index: usize) -> Self::Value {
        (
            self.crop_percent[index],
            self.grazing_percent[index],
            self.population_pressure_per_square_kilometer[index],
        )
    }
    fn width(&self) -> u8 {
        self.width
    }
    fn height(&self) -> u8 {
        self.height
    }
    fn hash(&self) -> Result<[u8; 32], GeodataError> {
        Ok(self.content_hash()?)
    }
}

pub(super) fn build_pyramids(
    sampler: &mut Sampler,
    stage: &Stage,
    samples_per_axis: u16,
    overview: &crate::PreparedOverview,
    hydrology: &crate::PreparedHydrology,
) -> Result<DetailedFields, GeodataError> {
    let mut elevation = Vec::new();
    let mut water = Vec::new();
    let mut vegetation = Vec::new();
    let mut historical_land_use = Vec::new();
    let mut axis = samples_per_axis;
    let mut previous_axis = axis;
    let mut level = 0_u8;
    loop {
        let count = usize::from(axis.div_ceil(PAGE));
        let mut elevation_root = PageRootBuilder::new(PageLayer::Elevation, count * count)?;
        let mut water_root = PageRootBuilder::new(PageLayer::Water, count * count)?;
        let mut vegetation_root = PageRootBuilder::new(PageLayer::Vegetation, count * count)?;
        let mut historical_root =
            PageRootBuilder::new(PageLayer::HistoricalLandUse, count * count)?;
        for y in 0..count {
            for x in 0..count {
                let elevation_page = if level == 0 {
                    sampler.page(axis, level, x as u16, y as u16)?
                } else {
                    reduce_elevation_page(stage, previous_axis, level, x as u16, y as u16)?
                };
                let elevation_hash = store_page(stage, &elevation_page, level, x as u16, y as u16)?;
                elevation_root.push(elevation_hash)?;
                let water_page: WaterPage = if level == 0 {
                    coarse_page(axis, level, x as u16, y as u16, |gx, gy| {
                        coarse_water(hydrology, overview, axis, gx, gy)
                    })?
                } else {
                    nearest_page::<WaterPage>(stage, previous_axis, level, x as u16, y as u16)?
                };
                let water_hash = store_page(stage, &water_page, level, x as u16, y as u16)?;
                water_root.push(water_hash)?;
                let vegetation_page: PotentialBiomePage = if level == 0 {
                    coarse_page(axis, level, x as u16, y as u16, |gx, gy| {
                        coarse_vegetation(overview, axis, gx, gy)
                    })?
                } else {
                    nearest_page::<PotentialBiomePage>(
                        stage,
                        previous_axis,
                        level,
                        x as u16,
                        y as u16,
                    )?
                };
                let vegetation_hash =
                    store_page(stage, &vegetation_page, level, x as u16, y as u16)?;
                vegetation_root.push(vegetation_hash)?;
                let historical_page: HistoricalLandUsePage = if level == 0 {
                    coarse_page(axis, level, x as u16, y as u16, |gx, gy| {
                        coarse_historical(overview, axis, gx, gy)
                    })?
                } else {
                    nearest_page::<HistoricalLandUsePage>(
                        stage,
                        previous_axis,
                        level,
                        x as u16,
                        y as u16,
                    )?
                };
                let historical_hash =
                    store_page(stage, &historical_page, level, x as u16, y as u16)?;
                historical_root.push(historical_hash)?;
            }
        }
        elevation.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: elevation_root.finish()?,
        });
        water.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: water_root.finish()?,
        });
        vegetation.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: vegetation_root.finish()?,
        });
        historical_land_use.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: historical_root.finish()?,
        });
        if axis == 1 {
            break;
        }
        previous_axis = axis;
        axis = axis.div_ceil(2);
        level = level.saturating_add(1);
    }
    Ok(DetailedFields {
        elevation: FieldPyramid { levels: elevation },
        water: FieldPyramid { levels: water },
        vegetation: FieldPyramid { levels: vegetation },
        historical_land_use: FieldPyramid {
            levels: historical_land_use,
        },
    })
}

fn store_page<P: PageOps>(
    stage: &Stage,
    page: &P,
    level: u8,
    x: u16,
    y: u16,
) -> Result<[u8; 32], GeodataError> {
    let bytes =
        serde_json::to_vec(page).map_err(|error| GeodataError::Directory(error.to_string()))?;
    stage.write(P::layer(), level, x, y, &bytes)?;
    page.hash()
}

fn coarse_page<P: PageOps>(
    axis: u16,
    level: u8,
    x: u16,
    y: u16,
    sample: impl Fn(u16, u16) -> Result<P::Value, GeodataError>,
) -> Result<P, GeodataError> {
    let width = (axis - x * PAGE).min(PAGE) as u8;
    let height = (axis - y * PAGE).min(PAGE) as u8;
    let mut values = Vec::with_capacity(usize::from(width) * usize::from(height));
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            values.push(sample(x * PAGE + column, y * PAGE + row)?);
        }
    }
    Ok(P::from_values(level, x, y, width, height, values))
}

pub(super) fn reduce_elevation_page(
    stage: &Stage,
    previous_axis: u16,
    level: u8,
    x: u16,
    y: u16,
) -> Result<ElevationPage, GeodataError> {
    let axis = previous_axis.div_ceil(2);
    let width = (axis - x * PAGE).min(PAGE) as u8;
    let height = (axis - y * PAGE).min(PAGE) as u8;
    let mut cache = BTreeMap::new();
    let mut values = Vec::with_capacity(usize::from(width) * usize::from(height));
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            let global_x = x * PAGE + column;
            let global_y = y * PAGE + row;
            let mut total = 0_i64;
            let mut count = 0_i64;
            for dy in 0..2 {
                for dx in 0..2 {
                    let source_x = global_x * 2 + dx;
                    let source_y = global_y * 2 + dy;
                    if source_x >= previous_axis || source_y >= previous_axis {
                        continue;
                    }
                    let key = (source_x / PAGE, source_y / PAGE);
                    let page = if let Some(page) = cache.get(&key) {
                        page
                    } else {
                        let bytes = stage.read(PageLayer::Elevation, level - 1, key.0, key.1)?;
                        let page: ElevationPage = serde_json::from_slice(&bytes)
                            .map_err(|error| GeodataError::Directory(error.to_string()))?;
                        cache.insert(key, page);
                        cache
                            .get(&key)
                            .ok_or(GeodataError::Preparation("elevation page cache miss"))?
                    };
                    let local_x = usize::from(source_x % PAGE).min(usize::from(page.width) - 1);
                    let local_y = usize::from(source_y % PAGE).min(usize::from(page.height) - 1);
                    total += i64::from(
                        page.geographic_height_centimeters
                            [local_y * usize::from(page.width) + local_x],
                    );
                    count += 1;
                }
            }
            let rounded = if total < 0 {
                (total - count / 2) / count
            } else {
                (total + count / 2) / count
            };
            values.push(rounded as i32);
        }
    }
    Ok(<ElevationPage as PageOps>::from_values(
        level, x, y, width, height, values,
    ))
}

pub(super) fn nearest_page<P: PageOps>(
    stage: &Stage,
    previous_axis: u16,
    level: u8,
    x: u16,
    y: u16,
) -> Result<P, GeodataError> {
    let axis = previous_axis.div_ceil(2);
    let width = (axis - x * PAGE).min(PAGE) as u8;
    let height = (axis - y * PAGE).min(PAGE) as u8;
    let mut cache = BTreeMap::new();
    let mut values = Vec::with_capacity(usize::from(width) * usize::from(height));
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            let source_x = (x * PAGE + column) * 2;
            let source_y = (y * PAGE + row) * 2;
            let key = (source_x / PAGE, source_y / PAGE);
            let page = if let Some(page) = cache.get(&key) {
                page
            } else {
                let bytes = stage.read(P::layer(), level - 1, key.0, key.1)?;
                let page: P = serde_json::from_slice(&bytes)
                    .map_err(|error| GeodataError::Directory(error.to_string()))?;
                cache.insert(key, page);
                cache
                    .get(&key)
                    .ok_or(GeodataError::Preparation("source page cache miss"))?
            };
            let local_x = usize::from(source_x % PAGE).min(usize::from(page.width()) - 1);
            let local_y = usize::from(source_y % PAGE).min(usize::from(page.height()) - 1);
            values.push(page.value(local_y * usize::from(page.width()) + local_x));
        }
    }
    Ok(P::from_values(level, x, y, width, height, values))
}

#[cfg(test)]
#[path = "tests/pyramid.rs"]
mod pyramid_tests;

pub(super) fn coarse_coordinate(axis: u16, value: u16) -> u16 {
    let numerator = (u32::from(value) * 2 + 1) * 128;
    let denominator = u32::from(axis) * 2;
    (numerator / denominator).min(127) as u16
}

fn coarse_vegetation(
    overview: &crate::PreparedOverview,
    axis: u16,
    x: u16,
    y: u16,
) -> Result<u8, GeodataError> {
    let x = coarse_coordinate(axis, x);
    let y = coarse_coordinate(axis, y);
    let page = overview
        .vegetation_pages
        .iter()
        .find(|page| page.level == 0 && page.x == x / PAGE && page.y == y / PAGE)
        .ok_or(GeodataError::Preparation(
            "overview vegetation page is missing",
        ))?;
    Ok(page.potential_biome_class
        [usize::from(y % PAGE) * usize::from(page.width) + usize::from(x % PAGE)])
}

fn coarse_historical(
    overview: &crate::PreparedOverview,
    axis: u16,
    x: u16,
    y: u16,
) -> Result<(u8, u8, u16), GeodataError> {
    let x = coarse_coordinate(axis, x);
    let y = coarse_coordinate(axis, y);
    let page = overview
        .historical_land_use_pages
        .iter()
        .find(|page| page.level == 0 && page.x == x / PAGE && page.y == y / PAGE)
        .ok_or(GeodataError::Preparation(
            "overview historical page is missing",
        ))?;
    let index = usize::from(y % PAGE) * usize::from(page.width) + usize::from(x % PAGE);
    Ok((
        page.crop_percent[index],
        page.grazing_percent[index],
        page.population_pressure_per_square_kilometer[index],
    ))
}
