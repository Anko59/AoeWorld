use super::super::RiverTopologyGrid;
use super::{ElevationGrid, GeodataError, HydrologyKind, WaterFlowDirection, WaterModelProvenance};

pub(super) fn model_river_surfaces(
    axis: u16,
    kinds: &[HydrologyKind],
    elevation: &ElevationGrid,
    topology: Option<&RiverTopologyGrid>,
    levels: &mut [Option<i32>],
    flow_directions: &mut [WaterFlowDirection],
    provenance: &mut [WaterModelProvenance],
) -> Result<(), GeodataError> {
    let len = usize::from(axis).pow(2);
    if kinds.len() != len
        || levels.len() != len
        || flow_directions.len() != len
        || provenance.len() != len
        || topology.is_some_and(|topology| topology.cells.len() != len)
    {
        return Err(GeodataError::Preparation(
            "river model arrays have inconsistent lengths",
        ));
    }
    for (index, kind) in kinds.iter().enumerate() {
        if *kind != HydrologyKind::River {
            continue;
        }
        let x = (index % usize::from(axis)) as u16;
        let y = (index / usize::from(axis)) as u16;
        if levels[index].is_none() {
            levels[index] = Some(elevation.target_height(axis, x, y));
            provenance[index] = WaterModelProvenance::ModelledRiverSurface;
        }
    }

    let Some(topology) = topology else {
        return Ok(());
    };
    let mut downstream: Vec<Option<usize>> = vec![None; len];
    for (index, kind) in kinds.iter().enumerate() {
        if *kind != HydrologyKind::River {
            continue;
        }
        let Some(source) = topology.cells[index] else {
            continue;
        };
        let x = (index % usize::from(axis)) as u16;
        let y = (index / usize::from(axis)) as u16;
        let mut best: Option<(u64, usize)> = None;
        for (neighbor_x, neighbor_y) in neighbors8(axis, x, y) {
            let neighbor = usize::from(neighbor_y) * usize::from(axis) + usize::from(neighbor_x);
            if kinds[neighbor] != HydrologyKind::River {
                continue;
            }
            let Some(target) = topology.cells[neighbor] else {
                continue;
            };
            let same_reach = target.reach_id == source.reach_id;
            let next_reach = source.next_down_id != 0 && target.reach_id == source.next_down_id;
            if !(same_reach || next_reach)
                || target.distance_to_sink_centimeters >= source.distance_to_sink_centimeters
            {
                continue;
            }
            let candidate = (target.distance_to_sink_centimeters, neighbor);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        if let Some((_, neighbor)) = best {
            downstream[index] = Some(neighbor);
            flow_directions[index] = direction_between(axis, index, neighbor)
                .ok_or(GeodataError::Preparation("river direction is invalid"))?;
        }
    }

    // HydroRIVERS' distance-to-downstream field gives a stable order. Process
    // downstream cells first, raising only upstream cells that would otherwise
    // create an uphill water profile. Unsupported edges retain their DEM level.
    let mut ordered = topology
        .cells
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| cell.map(|cell| (cell.distance_to_sink_centimeters, index)))
        .collect::<Vec<_>>();
    ordered.sort_unstable();
    for (_, index) in ordered {
        let Some(downstream_index) = downstream[index] else {
            continue;
        };
        if let (Some(upstream_level), Some(downstream_level)) =
            (levels[index], levels[downstream_index])
        {
            if upstream_level < downstream_level
                && matches!(
                    provenance[index],
                    WaterModelProvenance::GeographicCorrection
                        | WaterModelProvenance::ModelledJunctionSurface
                )
            {
                // A cited correction or lake/ocean junction is an anchor. Do
                // not move it to satisfy a contradictory neighboring profile.
                flow_directions[index] = WaterFlowDirection::Unknown;
                downstream[index] = None;
            } else {
                levels[index] = Some(upstream_level.max(downstream_level));
            }
        }
    }
    Ok(())
}

fn direction_between(axis: u16, source: usize, destination: usize) -> Option<WaterFlowDirection> {
    let source_x = (source % usize::from(axis)) as i32;
    let source_y = (source / usize::from(axis)) as i32;
    let destination_x = (destination % usize::from(axis)) as i32;
    let destination_y = (destination / usize::from(axis)) as i32;
    let dx = (destination_x - source_x).signum();
    let dy = (destination_y - source_y).signum();
    match (dx, dy) {
        (0, -1) => Some(WaterFlowDirection::North),
        (1, -1) => Some(WaterFlowDirection::NorthEast),
        (1, 0) => Some(WaterFlowDirection::East),
        (1, 1) => Some(WaterFlowDirection::SouthEast),
        (0, 1) => Some(WaterFlowDirection::South),
        (-1, 1) => Some(WaterFlowDirection::SouthWest),
        (-1, 0) => Some(WaterFlowDirection::West),
        (-1, -1) => Some(WaterFlowDirection::NorthWest),
        _ => None,
    }
}

fn neighbors8(axis: u16, x: u16, y: u16) -> impl Iterator<Item = (u16, u16)> {
    let mut neighbors = [(u16::MAX, u16::MAX); 8];
    let mut len = 0;
    for offset_y in -1_i32..=1 {
        for offset_x in -1_i32..=1 {
            if offset_x == 0 && offset_y == 0 {
                continue;
            }
            let neighbor_x = i32::from(x) + offset_x;
            let neighbor_y = i32::from(y) + offset_y;
            if neighbor_x >= 0
                && neighbor_y >= 0
                && neighbor_x < i32::from(axis)
                && neighbor_y < i32::from(axis)
            {
                neighbors[len] = (neighbor_x as u16, neighbor_y as u16);
                len += 1;
            }
        }
    }
    neighbors.into_iter().take(len)
}
