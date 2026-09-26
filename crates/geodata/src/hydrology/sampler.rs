use super::{
    GeodataError, HydrologyPage, MapRequest, ModernLandCoverPage, OpenTile, PAGE,
    RiverTopologyGrid, Tile, WORLD_COVER_NODATA, WORLD_COVER_PERMANENT_WATER, WORLD_COVER_WETLAND,
    hydrology_sampling::{
        open_vector_source, page_bounds, sample_worldcover_page, vector_features,
    },
};
use aoe_map::{HydrologyEvidenceMethod, HydrologyKind};
use gdal::{
    Dataset,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
    vector::Geometry,
};
use std::{collections::BTreeMap, path::Path};

#[path = "sampler/topology.rs"]
mod topology;
use topology::{
    RiverCellProjection, RiverReachMetadata, meters_to_centimeters, nearest_river_reach,
};

pub(super) struct Sampler {
    request: MapRequest,
    axis: u16,
    tiles: Vec<OpenTile>,
    ocean: Vec<u8>,
    lakes: Dataset,
    rivers: Option<Dataset>,
    river_reaches: BTreeMap<u32, RiverReachMetadata>,
    river_cells: Vec<Option<RiverCellProjection>>,
    river_topology: Option<RiverTopologyGrid>,
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
            river_reaches: BTreeMap::new(),
            river_cells: vec![None; usize::from(axis).pow(2)],
            river_topology: None,
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
        self.river_topology = self.resolve_river_topology()?;
        Ok((hydrology, land_cover))
    }

    pub(super) fn take_river_topology(
        &mut self,
    ) -> Result<Option<RiverTopologyGrid>, GeodataError> {
        Ok(self.river_topology.take())
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
        for feature in &river_features {
            if let Some(reach) = &feature.river_reach {
                self.record_reach(reach)?;
            }
        }
        let classes = sample_worldcover_page(&self.tiles, &longitude, &latitude)?;
        let mut kinds = Vec::with_capacity(east.len());
        let mut methods = Vec::with_capacity(east.len());
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
            if river
                && let Some((feature, station)) =
                    nearest_river_reach(&river_features, &point, local_east, local_north)
                && let Some(reach) = feature.river_reach.as_ref()
            {
                let offset = meters_to_centimeters(station).ok_or(GeodataError::Preparation(
                    "HydroRIVERS line distance is invalid",
                ))?;
                self.river_cells[global_index] = Some(RiverCellProjection {
                    reach_id: reach.id,
                    distance_from_start_centimeters: offset,
                });
            }
            let (kind, method) =
                classify_evidence(ocean, lake.map(|feature| feature.kind), river, class);
            kinds.push(kind as u8);
            methods.push(method as u8);
        }
        Ok((
            HydrologyPage {
                level: 0,
                x: x / PAGE,
                y: y / PAGE,
                width,
                height,
                kind: kinds,
                method: methods,
                water_model: None,
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

fn classify_evidence(
    ocean: bool,
    lake: Option<HydrologyKind>,
    river: bool,
    worldcover_class: u8,
) -> (HydrologyKind, HydrologyEvidenceMethod) {
    if ocean {
        (HydrologyKind::Ocean, HydrologyEvidenceMethod::OverviewOcean)
    } else if let Some(kind) = lake {
        (kind, HydrologyEvidenceMethod::HydroLakesExtent)
    } else if river {
        (
            HydrologyKind::River,
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
        )
    } else if matches!(worldcover_class, WORLD_COVER_WETLAND | 95) {
        (
            HydrologyKind::Shallow,
            HydrologyEvidenceMethod::WorldCoverClass,
        )
    } else if worldcover_class == WORLD_COVER_PERMANENT_WATER {
        (
            HydrologyKind::UnknownWater,
            HydrologyEvidenceMethod::WorldCoverClass,
        )
    } else if worldcover_class == WORLD_COVER_NODATA {
        (HydrologyKind::NoEvidence, HydrologyEvidenceMethod::None)
    } else {
        (
            HydrologyKind::Land,
            HydrologyEvidenceMethod::WorldCoverClass,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_precedence_keeps_modern_water_observations_typed() {
        assert_eq!(
            classify_evidence(true, Some(HydrologyKind::Lake), true, 40),
            (HydrologyKind::Ocean, HydrologyEvidenceMethod::OverviewOcean)
        );
        assert_eq!(
            classify_evidence(false, Some(HydrologyKind::RegulatedLake), true, 80),
            (
                HydrologyKind::RegulatedLake,
                HydrologyEvidenceMethod::HydroLakesExtent
            )
        );
        assert_eq!(
            classify_evidence(false, None, true, 40),
            (
                HydrologyKind::River,
                HydrologyEvidenceMethod::HydroRiversBufferedCorridor
            )
        );
        for class in [WORLD_COVER_WETLAND, 95] {
            assert_eq!(
                classify_evidence(false, None, false, class),
                (
                    HydrologyKind::Shallow,
                    HydrologyEvidenceMethod::WorldCoverClass
                )
            );
        }
        assert_eq!(
            classify_evidence(false, None, false, WORLD_COVER_PERMANENT_WATER),
            (
                HydrologyKind::UnknownWater,
                HydrologyEvidenceMethod::WorldCoverClass
            )
        );
        assert_eq!(
            classify_evidence(false, None, false, WORLD_COVER_NODATA),
            (HydrologyKind::NoEvidence, HydrologyEvidenceMethod::None)
        );
        assert_eq!(
            classify_evidence(false, None, false, 40),
            (
                HydrologyKind::Land,
                HydrologyEvidenceMethod::WorldCoverClass
            )
        );
    }
}
