use super::{DIAGONAL_COST, ORTHOGONAL_COST, Path};
use crate::{EnvironmentPageError, GroundMaterial, MapChunkGenerator};
use aoe_core::TileCoord;

pub(crate) fn segment_path_checked(
    terrain: &MapChunkGenerator,
    mut path: Path,
    segment_tiles: Option<u32>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Path, EnvironmentPageError> {
    let Some(limit) = segment_tiles else {
        return Ok(path);
    };
    let keep = usize::try_from(limit)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    if path.tiles.len() <= keep {
        return Ok(path);
    }
    path.tiles.truncate(keep);
    path.cost = path
        .tiles
        .windows(2)
        .map(|pair| checked_movement_cost(terrain, pair[0], pair[1], cancelled))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(u64::from)
        .sum();
    Ok(path)
}

fn checked_movement_cost(
    terrain: &MapChunkGenerator,
    from: TileCoord,
    to: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<u32, EnvironmentPageError> {
    let base = if from.x == to.x || from.y == to.y {
        ORTHOGONAL_COST
    } else {
        DIAGONAL_COST
    };
    let multiplier = matches!(
        terrain
            .tile_at_with_cancel(to, cancelled)?
            .map(|sample| sample.material),
        Some(GroundMaterial::Mud)
    )
    .then_some(3_u32)
    .unwrap_or(2);
    Ok(base * multiplier / 2)
}
