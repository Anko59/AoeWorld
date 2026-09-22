use super::{
    GeodataError, HydrologyKind, HydrologyPage, MapRequest, ModernLandCoverPage, OpenTile, PAGE,
    Tile, WORLD_COVER_NODATA, WORLD_COVER_PERMANENT_WATER, WORLD_COVER_WETLAND,
    hydrology_sampling::{
        open_vector_source, page_bounds, sample_worldcover_page, vector_features,
    },
};
use gdal::{
    Dataset,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
    vector::Geometry,
};
use std::path::Path;

pub(super) struct Sampler {
    request: MapRequest,
    axis: u16,
    tiles: Vec<OpenTile>,
    ocean: Vec<u8>,
    lakes: Dataset,
    rivers: Option<Dataset>,
}

impl Sampler {
    pub(super) fn new(
        request: MapRequest,
        axis: u16,
        tiles: Vec<Tile>,
        ocean: Vec<u8>,
        lakes_path: &Path,
        rivers_path: Option<&Path>,
    ) -> Result<Self, GeodataError> {
        if tiles.is_empty() {
            return Err(GeodataError::Preparation(
                "no WorldCover tile intersects the request",
            ));
        }
        if ocean.len() != usize::from(axis).pow(2) {
            return Err(GeodataError::Preparation(
                "resampled ocean coverage does not match the hydrology grid",
            ));
        }
        let tiles = tiles
            .into_iter()
            .map(|tile| {
                Ok(OpenTile {
                    latitude: tile.latitude,
                    longitude: tile.longitude,
                    dataset: Dataset::open(&tile.path)?,
                })
            })
            .collect::<Result<Vec<_>, GeodataError>>()?;
        let lakes = open_vector_source(lakes_path, super::LAKES_MEMBER)?;
        let rivers = rivers_path
            .map(|path| open_vector_source(path, super::RIVERS_MEMBER))
            .transpose()?;
        Ok(Self {
            request,
            axis,
            tiles,
            ocean,
            lakes,
            rivers,
        })
    }

    pub(super) fn pages(
        &mut self,
    ) -> Result<(Vec<HydrologyPage>, Vec<ModernLandCoverPage>), GeodataError> {
        let mut hydrology = Vec::new();
        let mut land_cover = Vec::new();
        for y in (0..self.axis).step_by(usize::from(PAGE)) {
            for x in (0..self.axis).step_by(usize::from(PAGE)) {
                let (water, modern) = self.page(x, y)?;
                hydrology.push(water);
                land_cover.push(modern);
            }
        }
        Ok((hydrology, land_cover))
    }

    fn page(
        &mut self,
        x: u16,
        y: u16,
    ) -> Result<(HydrologyPage, ModernLandCoverPage), GeodataError> {
        let width = (self.axis - x).min(PAGE) as u8;
        let height = (self.axis - y).min(PAGE) as u8;
        let side = self
            .request
            .estimate()
            .map_err(|_| GeodataError::Preparation("invalid request estimate"))?
            .effective_side_meters as f64;
        let spacing = side / f64::from(self.axis);
        let definition = crate::local_aeqd_definition(
            self.request.center_latitude_e7,
            self.request.center_longitude_e7,
        );
        let mut target =
            SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
        let mut wgs84 = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
        target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        wgs84.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
        let transform =
            CoordTransform::new(&target, &wgs84).map_err(|_| GeodataError::Projection)?;
        let mut east = Vec::with_capacity(usize::from(width) * usize::from(height));
        let mut north = Vec::with_capacity(east.capacity());
        for row in 0..u16::from(height) {
            for column in 0..u16::from(width) {
                east.push(-side / 2.0 + (f64::from(x + column) + 0.5) * spacing);
                north.push(side / 2.0 - (f64::from(y + row) + 0.5) * spacing);
            }
        }
        let mut longitude = east.clone();
        let mut latitude = north.clone();
        transform
            .transform_coords(&mut longitude, &mut latitude, &mut [])
            .map_err(|_| GeodataError::Coordinate)?;
        let page_bounds = page_bounds(&longitude, &latitude)?;
        let lake_features = vector_features(&mut self.lakes, page_bounds, &definition, false)?;
        let river_features = if let Some(rivers) = &mut self.rivers {
            vector_features(rivers, page_bounds, &definition, true)?
        } else {
            Vec::new()
        };
        let classes = sample_worldcover_page(&self.tiles, &longitude, &latitude)?;
        let mut kinds = Vec::with_capacity(east.len());
        for (index, (&local_east, &local_north)) in east.iter().zip(&north).enumerate() {
            let class = classes[index];
            let point = Geometry::from_wkt(&format!("POINT ({local_east} {local_north})"))
                .map_err(|_| GeodataError::Preparation("could not construct sample point"))?;
            let global_index = (usize::from(y) + index / usize::from(width))
                * usize::from(self.axis)
                + usize::from(x)
                + index % usize::from(width);
            let ocean = self.ocean[global_index] >= 50;
            let lake = lake_features
                .iter()
                .find(|feature| feature.geometry.contains(&point));
            let river = lake.is_none()
                && river_features
                    .iter()
                    .any(|feature| feature.geometry.contains(&point));
            let kind = if ocean {
                HydrologyKind::Ocean
            } else if let Some(feature) = lake {
                feature.kind
            } else if river {
                HydrologyKind::River
            } else if class == WORLD_COVER_WETLAND {
                HydrologyKind::Shallow
            } else if class == WORLD_COVER_PERMANENT_WATER {
                HydrologyKind::UnknownWater
            } else if class == WORLD_COVER_NODATA {
                HydrologyKind::NoEvidence
            } else {
                HydrologyKind::Land
            };
            kinds.push(kind as u8);
        }
        let size = kinds.len();
        Ok((
            HydrologyPage {
                level: 0,
                x: x / PAGE,
                y: y / PAGE,
                width,
                height,
                kind: kinds,
                surface_height_centimeters: vec![0; size],
                surface_height_known: vec![0; size.div_ceil(8)],
                barrier_edges: vec![0; size],
            },
            ModernLandCoverPage {
                level: 0,
                x: x / PAGE,
                y: y / PAGE,
                width,
                height,
                worldcover_class: classes,
            },
        ))
    }
}
