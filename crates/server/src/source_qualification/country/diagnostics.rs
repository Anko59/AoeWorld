//! Observations only: straight-line failure does not explain planner failure.
use crate::PageResidency;
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, EdgePassability, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, GroundMaterial, HydrologyObservation,
    LandscapeAppearance, MapChunkGenerator, MapPackage, PageLayer, ResourceNode, Tile, WaterKind,
};
use aoe_simulation::Terrain;
use serde::Serialize;

pub(super) const LOCAL_DISTANCE: i32 = 128;
pub(super) const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const NEIGHBOURHOOD_RADIUS: i32 = 2;

#[derive(Clone, Debug, Serialize)]
pub struct TileObservation {
    pub coordinate: [i32; 2],
    /// Immutable physical terrain; material may include landscape composition.
    /// This is not a pre-composition base-tile query or source-grid elevation.
    pub raw_physical: Tile,
    pub effective_passable: bool,
    pub resource: Option<ResourceNode>,
    pub appearance: Option<LandscapeAppearance>,
    pub clearing_reservations: ClearingReservations,
    pub overview_water: Option<OverviewWaterObservation>,
    pub source_hydrology_coordinate: Option<[u16; 2]>,
    pub source_hydrology: Option<HydrologyObservation>,
    pub water_model: Option<WaterModelObservation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClearingReservations {
    pub route: bool,
    pub start: bool,
    pub resource_approach: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct OverviewWaterObservation {
    pub source_coordinate: [u16; 2],
    pub ocean_coverage_percent: u8,
    pub inland_coverage_percent: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct WaterModelObservation {
    pub source_coordinate: [u16; 2],
    pub kind_code: u8,
    pub surface_level_centimeters: Option<i32>,
    pub flow_direction_code: u8,
    pub provenance_code: u8,
}

#[derive(Debug, Serialize)]
pub struct StraightLineProbe {
    pub interpretation: &'static str,
    pub steps: Vec<TransitionObservation>,
    /// Counts refer to 128 destination samples, excluding the origin.
    pub destination_counts: TileCounts,
    pub blocked_physical_edges: usize,
    pub blocked_authoritative_transitions: usize,
}

#[derive(Debug, Serialize)]
pub struct TransitionObservation {
    pub step: usize,
    pub from: [i32; 2],
    pub to: TileObservation,
    pub physical_edge_crossable: bool,
    pub authoritative_crossable: bool,
    pub blockers: TransitionBlockers,
}

/// Independent observations, not mutually exclusive or inferred root causes.
#[derive(Debug, Serialize)]
pub struct TransitionBlockers {
    pub from_raw_impassable: bool,
    pub to_raw_impassable: bool,
    pub from_nonwalkable_surface: bool,
    pub to_nonwalkable_surface: bool,
    pub game_height_step_exceeds_one: bool,
    pub to_effective_impassable: bool,
    pub to_resource_present: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct TileCounts {
    pub sampled_tiles: usize,
    pub water_present: usize,
    pub nonwalkable_surface: usize,
    pub raw_impassable: usize,
    pub effective_impassable: usize,
    pub resource_present: usize,
    pub forest_floor: usize,
    pub canopy_present: usize,
}

impl TileCounts {
    pub(super) fn observe(&mut self, sample: &TileObservation) {
        self.sampled_tiles += 1;
        self.water_present += usize::from(sample.raw_physical.water != WaterKind::None);
        self.nonwalkable_surface += usize::from(!sample.raw_physical.surface.walkable());
        self.raw_impassable += usize::from(!sample.raw_physical.passable);
        self.effective_impassable += usize::from(!sample.effective_passable);
        self.resource_present += usize::from(sample.resource.is_some());
        self.forest_floor +=
            usize::from(sample.raw_physical.material == GroundMaterial::ForestFloor);
        self.canopy_present += usize::from(
            sample
                .appearance
                .is_some_and(|appearance| appearance.canopy_strength > 0),
        );
    }
}

pub(super) struct Diagnostics<'a> {
    pub terrain: &'a Terrain,
    pub generator: &'a MapChunkGenerator,
    pub config: WorldConfig,
    pub package: &'a MapPackage,
    pub provider: &'a PageResidency,
}

impl Diagnostics<'_> {
    pub fn neighbourhood(
        &self,
        center: TileCoord,
    ) -> Result<Vec<TileObservation>, EnvironmentPageError> {
        let mut tiles = Vec::with_capacity(25);
        for dy in -NEIGHBOURHOOD_RADIUS..=NEIGHBOURHOOD_RADIUS {
            for dx in -NEIGHBOURHOOD_RADIUS..=NEIGHBOURHOOD_RADIUS {
                tiles.push(self.tile(TileCoord::new(center.x + dx, center.y + dy))?);
            }
        }
        Ok(tiles)
    }

    pub fn straight_line(
        &self,
        origin: TileCoord,
        direction: (i32, i32),
    ) -> Result<StraightLineProbe, EnvironmentPageError> {
        let mut from = self.tile(origin)?;
        let mut steps = Vec::with_capacity(LOCAL_DISTANCE as usize);
        let mut destination_counts = TileCounts::default();
        let mut blocked_physical_edges = 0;
        let mut blocked_authoritative_transitions = 0;
        for (step, to_coord) in straight_destinations(origin, direction) {
            let from_coord = TileCoord::new(from.coordinate[0], from.coordinate[1]);
            let to = self.tile(to_coord)?;
            let physical_edge_crossable = matches!(
                self.generator
                    .edge_between_with_cancel(from_coord, to_coord, &|| false)?,
                EdgePassability::Passable
            );
            let authoritative_crossable =
                self.terrain
                    .crossable_with_cancel(from_coord, to_coord, self.config, &|| false)?;
            let blockers = blockers(&from, &to);
            destination_counts.observe(&to);
            blocked_physical_edges += usize::from(!physical_edge_crossable);
            blocked_authoritative_transitions += usize::from(!authoritative_crossable);
            let next = to.clone();
            steps.push(TransitionObservation {
                step: step as usize,
                from: from.coordinate,
                to,
                physical_edge_crossable,
                authoritative_crossable,
                blockers,
            });
            from = next;
        }
        Ok(StraightLineProbe {
            interpretation: "fixed-cardinal-observations-not-planner-root-cause; counts-overlap; water-source-evidence-is-not-physical-slope-or-forest",
            steps,
            destination_counts,
            blocked_physical_edges,
            blocked_authoritative_transitions,
        })
    }

    fn tile(&self, coord: TileCoord) -> Result<TileObservation, EnvironmentPageError> {
        let point = self
            .generator
            .landscape_point_with_cancel(coord, &|| false)?
            .ok_or(EnvironmentPageError::Invalid)?;
        let reservations = self.generator.landscape_reservations_at(coord);
        let (source_hydrology, water_model) = self.hydrology(coord)?;
        Ok(TileObservation {
            coordinate: [coord.x, coord.y],
            raw_physical: point.tile.terrain,
            effective_passable: self
                .terrain
                .passable_with_cancel(coord, self.config, &|| false)?,
            resource: point.resource.map(|resource| resource.node),
            appearance: point.tile.appearance,
            clearing_reservations: ClearingReservations {
                route: reservations.route,
                start: reservations.start,
                resource_approach: reservations.resource_approach,
            },
            overview_water: self.overview_water(coord)?,
            source_hydrology_coordinate: self.package.environment.hydrology_evidence.as_ref().map(
                |index| nearest_source(coord, index.samples_per_axis, self.config.width_tiles),
            ),
            source_hydrology,
            water_model,
        })
    }

    fn overview_water(
        &self,
        coord: TileCoord,
    ) -> Result<Option<OverviewWaterObservation>, EnvironmentPageError> {
        let Some(axis) = self.package.environment.water_samples_per_axis() else {
            return Ok(None);
        };
        let [x, y] = nearest_source(coord, axis, self.config.width_tiles);
        let page_axis = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let page = self.provider.page(
            EnvironmentPageKey {
                layer: PageLayer::Water,
                level: 0,
                x: x / page_axis,
                y: y / page_axis,
            },
            &|| false,
        )?;
        let EnvironmentPage::Water(page) = page.as_ref() else {
            return Err(EnvironmentPageError::Corrupt);
        };
        let offset =
            usize::from(y % page_axis) * usize::from(page.width) + usize::from(x % page_axis);
        Ok(Some(OverviewWaterObservation {
            source_coordinate: [x, y],
            ocean_coverage_percent: *page
                .ocean_coverage_percent
                .get(offset)
                .ok_or(EnvironmentPageError::Corrupt)?,
            inland_coverage_percent: *page
                .inland_coverage_percent
                .get(offset)
                .ok_or(EnvironmentPageError::Corrupt)?,
        }))
    }

    fn hydrology(
        &self,
        coord: TileCoord,
    ) -> Result<(Option<HydrologyObservation>, Option<WaterModelObservation>), EnvironmentPageError>
    {
        let Some(index) = &self.package.environment.hydrology_evidence else {
            return Ok((None, None));
        };
        let [x, y] = nearest_source(coord, index.samples_per_axis, self.config.width_tiles);
        let page_axis = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let page = self.provider.page(
            EnvironmentPageKey {
                layer: PageLayer::HydrologyEvidence,
                level: 0,
                x: x / page_axis,
                y: y / page_axis,
            },
            &|| false,
        )?;
        let EnvironmentPage::HydrologyEvidence(page) = page.as_ref() else {
            return Err(EnvironmentPageError::Corrupt);
        };
        let local_x = x % page_axis;
        let local_y = y % page_axis;
        if local_x >= u16::from(page.width) || local_y >= u16::from(page.height) {
            return Err(EnvironmentPageError::Corrupt);
        }
        let offset = usize::from(local_y) * usize::from(page.width) + usize::from(local_x);
        let observation = page
            .observation(offset)
            .map_err(|_| EnvironmentPageError::Corrupt)?;
        let model = page
            .water_model
            .as_ref()
            .map(|model| WaterModelObservation {
                source_coordinate: [x, y],
                kind_code: model.kind[offset],
                surface_level_centimeters: model.surface_level_centimeters[offset],
                flow_direction_code: model.flow_direction[offset],
                provenance_code: model.provenance[offset],
            });
        Ok((Some(observation), model))
    }
}

pub(super) fn straight_destinations(
    origin: TileCoord,
    direction: (i32, i32),
) -> impl Iterator<Item = (i32, TileCoord)> {
    (1..=LOCAL_DISTANCE).map(move |step| {
        (
            step,
            TileCoord::new(origin.x + direction.0 * step, origin.y + direction.1 * step),
        )
    })
}

// Matches provider/helpers.rs nearest-integer axis-minus-one mapping, not
// geodata pixel-center projection. Supported country axes/width are nonzero.
pub(super) fn nearest_source(coord: TileCoord, samples: u16, width_tiles: i32) -> [u16; 2] {
    let source_axis = u64::from(samples - 1);
    let tile_axis = (width_tiles - 1) as u64;
    let axis = |value: i32| {
        ((value.clamp(0, width_tiles - 1) as u64 * source_axis + tile_axis / 2) / tile_axis) as u16
    };
    [axis(coord.x), axis(coord.y)]
}

pub(super) fn blockers(from: &TileObservation, to: &TileObservation) -> TransitionBlockers {
    TransitionBlockers {
        from_raw_impassable: !from.raw_physical.passable,
        to_raw_impassable: !to.raw_physical.passable,
        from_nonwalkable_surface: !from.raw_physical.surface.walkable(),
        to_nonwalkable_surface: !to.raw_physical.surface.walkable(),
        game_height_step_exceeds_one: (i32::from(from.raw_physical.game_height_level)
            - i32::from(to.raw_physical.game_height_level))
        .abs()
            > 1,
        to_effective_impassable: !to.effective_passable,
        to_resource_present: to.resource.is_some(),
    }
}
