use super::{
    GeodataError, HydrologyKind, HydrologyPage, MAX_HYDROLOGY_SAMPLES_PER_AXIS, PAGE,
    RiverTopologyGrid,
};
use aoe_map::{
    ElevationPage, HYDROLOGY_WATER_MODEL_VERSION, HydrologyWaterModelIndex,
    HydrologyWaterModelPage, MapRequest, WaterCorrectionDocument, WaterCorrectionOperation,
    WaterFlowDirection, WaterModelProvenance,
};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use std::collections::VecDeque;

#[path = "model/elevation.rs"]
mod elevation;
#[path = "model/river.rs"]
mod river;
pub(super) use elevation::ElevationGrid;
use river::model_river_surfaces;

/// Applies a bounded correction document, builds natural-lake components, and
/// attaches the model output to the original evidence pages. Lake levels are
/// estimated from the lower quartile of adjacent source DEM cells, resampled
/// to the evidence grid; this is a modeled surface, not a source
/// measurement. Directed river profiles are produced only where sampled reach
/// topology establishes a downstream path.
#[cfg(test)]
pub(super) fn prepare_water_model(
    request: MapRequest,
    pages: &mut [HydrologyPage],
    elevation_pages: &[ElevationPage],
    corrections: WaterCorrectionDocument,
) -> Result<HydrologyWaterModelIndex, GeodataError> {
    prepare_water_model_with_river_topology(request, pages, elevation_pages, corrections, None)
}

pub(super) fn prepare_water_model_with_river_topology(
    request: MapRequest,
    pages: &mut [HydrologyPage],
    elevation_pages: &[ElevationPage],
    corrections: WaterCorrectionDocument,
    river_topology: Option<&RiverTopologyGrid>,
) -> Result<HydrologyWaterModelIndex, GeodataError> {
    let axis = evidence_axis(pages)?;
    if !(2..=MAX_HYDROLOGY_SAMPLES_PER_AXIS).contains(&axis) {
        return Err(GeodataError::Preparation("invalid hydrology model axis"));
    }
    corrections
        .validate_for(request, axis)
        .map_err(|_| GeodataError::Preparation("water corrections do not match request grid"))?;
    let original = flatten_evidence(pages, axis)?;
    let mut kinds = original.clone();
    let mut provenance = vec![WaterModelProvenance::EvidenceOnly; original.len()];
    let elevation = ElevationGrid::new(elevation_pages)?;
    let definition =
        crate::local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    let polygons = project_corrections(&corrections, &definition)?;
    let effective_side = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid correction request"))?
        .effective_side_meters as f64;
    let spacing = effective_side / f64::from(axis);
    for (patch, polygon) in corrections.patches.iter().zip(&polygons) {
        let min_x = polygon
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let max_x = polygon
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = polygon
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let max_y = polygon
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let cell_x = |local_x: f64| ((local_x + effective_side / 2.0) / spacing).floor() as i32;
        let cell_y = |local_y: f64| ((effective_side / 2.0 - local_y) / spacing).floor() as i32;
        let x0 = cell_x(min_x)
            .saturating_sub(1)
            .clamp(0, i32::from(axis) - 1) as u16;
        let x1 = cell_x(max_x)
            .saturating_add(1)
            .clamp(0, i32::from(axis) - 1) as u16;
        let y0 = cell_y(max_y)
            .saturating_sub(1)
            .clamp(0, i32::from(axis) - 1) as u16;
        let y1 = cell_y(min_y)
            .saturating_add(1)
            .clamp(0, i32::from(axis) - 1) as u16;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let local_x = -effective_side / 2.0 + (f64::from(x) + 0.5) * spacing;
                let local_y = effective_side / 2.0 - (f64::from(y) + 0.5) * spacing;
                if point_in_polygon(local_x, local_y, polygon) {
                    let index = usize::from(y) * usize::from(axis) + usize::from(x);
                    let corrected_kind = match patch.operation {
                        WaterCorrectionOperation::SetNaturalLake => HydrologyKind::Lake,
                        WaterCorrectionOperation::SetLand => HydrologyKind::Land,
                    };
                    kinds[index] = corrected_kind;
                    provenance[index] = WaterModelProvenance::GeographicCorrection;
                }
            }
        }
    }

    let mut surface_levels = vec![None; kinds.len()];
    let mut flow_directions = vec![WaterFlowDirection::Unknown; kinds.len()];
    for (index, kind) in kinds.iter().enumerate() {
        if *kind == HydrologyKind::Ocean {
            surface_levels[index] = Some(0);
            if provenance[index] != WaterModelProvenance::GeographicCorrection {
                provenance[index] = WaterModelProvenance::ModelledOceanSurface;
            }
        }
    }
    let correction_provenance = provenance.clone();
    model_lake_components(
        axis,
        &kinds,
        &correction_provenance,
        &elevation,
        &mut surface_levels,
        &mut provenance,
    )?;
    model_lake_junctions(axis, &kinds, &mut surface_levels, &mut provenance);
    model_river_surfaces(
        axis,
        &kinds,
        &elevation,
        river_topology,
        &mut surface_levels,
        &mut flow_directions,
        &mut provenance,
    )?;

    for page in pages {
        let page_x = page.x * PAGE;
        let page_y = page.y * PAGE;
        let mut page_kind = Vec::with_capacity(page.kind.len());
        let mut page_levels = Vec::with_capacity(page.kind.len());
        let mut page_flow = Vec::with_capacity(page.kind.len());
        let mut page_provenance = Vec::with_capacity(page.kind.len());
        for local_y in 0..u16::from(page.height) {
            for local_x in 0..u16::from(page.width) {
                let x = page_x + local_x;
                let y = page_y + local_y;
                let index = usize::from(y) * usize::from(axis) + usize::from(x);
                page_kind.push(kinds[index] as u8);
                page_levels.push(surface_levels[index]);
                page_flow.push(flow_directions[index] as u8);
                page_provenance.push(provenance[index] as u8);
            }
        }
        page.water_model = Some(HydrologyWaterModelPage {
            kind: page_kind,
            surface_level_centimeters: page_levels,
            flow_direction: page_flow,
            provenance: page_provenance,
        });
    }
    Ok(HydrologyWaterModelIndex {
        model_version: HYDROLOGY_WATER_MODEL_VERSION,
        samples_per_axis: axis,
        target_year_ce: corrections.target_year_ce,
        correction_document: corrections,
    })
}

fn evidence_axis(pages: &[HydrologyPage]) -> Result<u16, GeodataError> {
    if pages.is_empty() {
        return Err(GeodataError::Preparation("hydrology evidence is empty"));
    }
    let axis_x = pages
        .iter()
        .map(|page| page.x * PAGE + u16::from(page.width))
        .max()
        .unwrap_or(0);
    let axis_y = pages
        .iter()
        .map(|page| page.y * PAGE + u16::from(page.height))
        .max()
        .unwrap_or(0);
    (axis_x == axis_y && axis_x > 0)
        .then_some(axis_x)
        .ok_or(GeodataError::Preparation(
            "hydrology evidence grid is not square",
        ))
}

fn flatten_evidence(
    pages: &[HydrologyPage],
    axis: u16,
) -> Result<Vec<HydrologyKind>, GeodataError> {
    let mut kinds = vec![HydrologyKind::NoEvidence; usize::from(axis).pow(2)];
    let page_columns = axis.div_ceil(PAGE);
    if pages.len() != usize::from(page_columns).pow(2) {
        return Err(GeodataError::Preparation(
            "hydrology evidence page set is incomplete",
        ));
    }
    let mut seen = vec![false; pages.len()];
    for page in pages {
        if page.width != (axis - page.x * PAGE).min(PAGE) as u8
            || page.height != (axis - page.y * PAGE).min(PAGE) as u8
        {
            return Err(GeodataError::Preparation(
                "hydrology evidence page shape is invalid",
            ));
        }
        let slot = usize::from(page.y) * usize::from(page_columns) + usize::from(page.x);
        let Some(present) = seen.get_mut(slot) else {
            return Err(GeodataError::Preparation(
                "hydrology evidence page is out of range",
            ));
        };
        if *present {
            return Err(GeodataError::Preparation(
                "hydrology evidence page is duplicated",
            ));
        }
        *present = true;
        for local_y in 0..u16::from(page.height) {
            for local_x in 0..u16::from(page.width) {
                let local = usize::from(local_y) * usize::from(page.width) + usize::from(local_x);
                let index = usize::from(page.y * PAGE + local_y) * usize::from(axis)
                    + usize::from(page.x * PAGE + local_x);
                kinds[index] = page.kind[local]
                    .try_into()
                    .map_err(|_| GeodataError::Preparation("invalid hydrology evidence kind"))?;
            }
        }
    }
    if seen.iter().any(|present| !present) {
        return Err(GeodataError::Preparation(
            "hydrology evidence page set is incomplete",
        ));
    }
    Ok(kinds)
}

fn project_corrections(
    corrections: &WaterCorrectionDocument,
    target_definition: &str,
) -> Result<Vec<Vec<(f64, f64)>>, GeodataError> {
    let mut source = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    let mut target =
        SpatialRef::from_definition(target_definition).map_err(|_| GeodataError::Projection)?;
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&source, &target).map_err(|_| GeodataError::Projection)?;
    corrections
        .patches
        .iter()
        .map(|patch| {
            let mut x = patch
                .polygon
                .iter()
                .map(|vertex| f64::from(vertex.longitude_e7) / 10_000_000.0)
                .collect::<Vec<_>>();
            let mut y = patch
                .polygon
                .iter()
                .map(|vertex| f64::from(vertex.latitude_e7) / 10_000_000.0)
                .collect::<Vec<_>>();
            transform
                .transform_coords(&mut x, &mut y, &mut [])
                .map_err(|_| GeodataError::Coordinate)?;
            Ok(x.into_iter().zip(y).collect::<Vec<_>>())
        })
        .collect()
}

fn point_in_polygon(x: f64, y: f64, polygon: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let (current_x, current_y) = polygon[current];
        let (previous_x, previous_y) = polygon[previous];
        let crosses = (current_y > y) != (previous_y > y)
            && x < (previous_x - current_x) * (y - current_y) / (previous_y - current_y)
                + current_x;
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn model_lake_components(
    axis: u16,
    kinds: &[HydrologyKind],
    provenance: &[WaterModelProvenance],
    elevation: &ElevationGrid,
    levels: &mut [Option<i32>],
    output_provenance: &mut [WaterModelProvenance],
) -> Result<(), GeodataError> {
    let len = kinds.len();
    if len != usize::from(axis).pow(2)
        || provenance.len() != len
        || levels.len() != len
        || output_provenance.len() != len
    {
        return Err(GeodataError::Preparation(
            "water model arrays have inconsistent lengths",
        ));
    }
    let mut visited = vec![false; len];
    let mut queue = VecDeque::new();
    let mut component = Vec::new();
    let mut shore_heights = Vec::new();
    for start in 0..len {
        if kinds[start] != HydrologyKind::Lake || visited[start] {
            continue;
        }
        queue.clear();
        component.clear();
        shore_heights.clear();
        visited[start] = true;
        queue.push_back(start);
        while let Some(index) = queue.pop_front() {
            component.push(index);
            let x = (index % usize::from(axis)) as u16;
            let y = (index / usize::from(axis)) as u16;
            for (neighbor_x, neighbor_y) in neighbors(axis, x, y) {
                let neighbor =
                    usize::from(neighbor_y) * usize::from(axis) + usize::from(neighbor_x);
                if kinds[neighbor] == HydrologyKind::Lake {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                } else if matches!(
                    kinds[neighbor],
                    HydrologyKind::Land | HydrologyKind::Shallow
                ) {
                    shore_heights.push(elevation.target_height(axis, neighbor_x, neighbor_y));
                }
            }
        }
        if shore_heights.is_empty() {
            for index in &component {
                let x = (*index % usize::from(axis)) as u16;
                let y = (*index / usize::from(axis)) as u16;
                shore_heights.push(elevation.target_height(axis, x, y));
            }
        }
        shore_heights.sort_unstable();
        let level = shore_heights[(shore_heights.len() - 1) / 4];
        for index in component.iter().copied() {
            levels[index] = Some(level);
            if provenance[index] != WaterModelProvenance::GeographicCorrection {
                output_provenance[index] = WaterModelProvenance::ModelledLakeSurface;
            }
        }
    }
    Ok(())
}

fn model_lake_junctions(
    axis: u16,
    kinds: &[HydrologyKind],
    levels: &mut [Option<i32>],
    provenance: &mut [WaterModelProvenance],
) {
    let mut junction_levels = vec![None; kinds.len()];
    for index in 0..kinds.len() {
        if kinds[index] != HydrologyKind::River {
            continue;
        }
        let x = (index % usize::from(axis)) as u16;
        let y = (index / usize::from(axis)) as u16;
        let mut adjacent = Vec::new();
        for (neighbor_x, neighbor_y) in neighbors(axis, x, y) {
            let neighbor = usize::from(neighbor_y) * usize::from(axis) + usize::from(neighbor_x);
            if matches!(kinds[neighbor], HydrologyKind::Lake | HydrologyKind::Ocean)
                && let Some(level) = levels[neighbor]
            {
                adjacent.push(level);
            }
        }
        adjacent.sort_unstable();
        adjacent.dedup();
        if adjacent.len() == 1 {
            junction_levels[index] = adjacent.first().copied();
        }
    }
    for (index, level) in junction_levels.into_iter().enumerate() {
        if let Some(level) = level {
            levels[index] = Some(level);
            provenance[index] = WaterModelProvenance::ModelledJunctionSurface;
        }
    }
}

fn neighbors(axis: u16, x: u16, y: u16) -> impl Iterator<Item = (u16, u16)> {
    let mut neighbors = [(u16::MAX, u16::MAX); 4];
    let mut len = 0;
    if x > 0 {
        neighbors[len] = (x - 1, y);
        len += 1;
    }
    if x + 1 < axis {
        neighbors[len] = (x + 1, y);
        len += 1;
    }
    if y > 0 {
        neighbors[len] = (x, y - 1);
        len += 1;
    }
    if y + 1 < axis {
        neighbors[len] = (x, y + 1);
        len += 1;
    }
    neighbors.into_iter().take(len)
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
