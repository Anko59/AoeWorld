use super::MapChunkGenerator;
use crate::EnvironmentPageError;
use crate::Ratio;
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};

pub const MAX_TRAVERSABLE_SOURCE_GRADE_PERCENT: u16 = 35;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SurfaceKind {
    Plateau,
    Ramp,
    Cliff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SurfaceDiagonal {
    NorthwestSoutheast,
    NortheastSouthwest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgePassability {
    Passable,
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TileSurface {
    /// Shared corners ordered northwest, northeast, southeast, southwest.
    pub corner_game_height_levels: [i16; 4],
    pub kind: SurfaceKind,
    pub triangulation: SurfaceDiagonal,
}

impl TileSurface {
    pub const fn walkable(self) -> bool {
        !matches!(self.kind, SurfaceKind::Cliff)
    }
}

impl MapChunkGenerator {
    /// Determines whether an adjacent terrain edge can be crossed without
    /// interpreting center-height samples as a substitute for shared geometry.
    pub fn edge_between(&self, from: TileCoord, to: TileCoord) -> EdgePassability {
        let delta_x = to.x.saturating_sub(from.x).unsigned_abs();
        let delta_y = to.y.saturating_sub(from.y).unsigned_abs();
        if (delta_x == 0 && delta_y == 0) || delta_x > 1 || delta_y > 1 {
            return EdgePassability::Blocked;
        }
        let Some(from) = self.tile_at(from) else {
            return EdgePassability::Blocked;
        };
        let Some(to) = self.tile_at(to) else {
            return EdgePassability::Blocked;
        };
        if !from.passable
            || !to.passable
            || !from.surface.walkable()
            || !to.surface.walkable()
            || (i32::from(from.game_height_level) - i32::from(to.game_height_level)).abs() > 1
        {
            return EdgePassability::Blocked;
        }
        EdgePassability::Passable
    }

    pub fn edge_between_with_cancel(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<EdgePassability, EnvironmentPageError> {
        let delta_x = to.x.saturating_sub(from.x).unsigned_abs();
        let delta_y = to.y.saturating_sub(from.y).unsigned_abs();
        if (delta_x == 0 && delta_y == 0) || delta_x > 1 || delta_y > 1 {
            return Ok(EdgePassability::Blocked);
        }
        let from = self
            .tile_at_with_cancel(from, cancelled)?
            .ok_or(EnvironmentPageError::Invalid)?;
        let to = self
            .tile_at_with_cancel(to, cancelled)?
            .ok_or(EnvironmentPageError::Invalid)?;
        Ok(
            if !from.passable
                || !to.passable
                || !from.surface.walkable()
                || !to.surface.walkable()
                || (i32::from(from.game_height_level) - i32::from(to.game_height_level)).abs() > 1
            {
                EdgePassability::Blocked
            } else {
                EdgePassability::Passable
            },
        )
    }
}

pub(super) fn from_heights(
    geographic_height_centimeters: [i32; 4],
    compression: Ratio,
) -> TileSurface {
    let corner_game_height_levels = geographic_height_centimeters.map(|height| {
        let height = i64::from(height).saturating_mul(i64::from(compression.denominator))
            / (100 * i64::from(compression.numerator));
        height.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
    });
    let minimum = *corner_game_height_levels.iter().min().unwrap_or(&0);
    let maximum = *corner_game_height_levels.iter().max().unwrap_or(&0);
    let source_grade_exceeded =
        has_excessive_source_grade(geographic_height_centimeters, compression);
    let kind = if minimum == maximum {
        SurfaceKind::Plateau
    } else if !source_grade_exceeded && i32::from(maximum) - i32::from(minimum) == 1 {
        SurfaceKind::Ramp
    } else {
        SurfaceKind::Cliff
    };
    let triangulation = if i32::from(corner_game_height_levels[0])
        + i32::from(corner_game_height_levels[2])
        <= i32::from(corner_game_height_levels[1]) + i32::from(corner_game_height_levels[3])
    {
        SurfaceDiagonal::NorthwestSoutheast
    } else {
        SurfaceDiagonal::NortheastSouthwest
    };
    TileSurface {
        corner_game_height_levels,
        kind,
        triangulation,
    }
}

fn has_excessive_source_grade(corners: [i32; 4], compression: Ratio) -> bool {
    let rise = [
        (i64::from(corners[0]) - i64::from(corners[1])).abs(),
        (i64::from(corners[1]) - i64::from(corners[2])).abs(),
        (i64::from(corners[2]) - i64::from(corners[3])).abs(),
        (i64::from(corners[3]) - i64::from(corners[0])).abs(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    rise.saturating_mul(i64::from(compression.denominator))
        .saturating_mul(100)
        > i64::from(MAX_TRAVERSABLE_SOURCE_GRADE_PERCENT)
            .saturating_mul(200)
            .saturating_mul(i64::from(compression.numerator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_level_patterns_are_ramps_and_saddles_have_a_stable_diagonal() {
        let ramp = from_heights([0, 100, 100, 0], Ratio::new(1, 1).expect("ratio"));
        assert_eq!(ramp.kind, SurfaceKind::Cliff);
        let ramp = from_heights(
            [2_999, 5_099, 5_099, 2_999],
            Ratio::new(30, 1).expect("ratio"),
        );
        assert_eq!(ramp.kind, SurfaceKind::Ramp);
        assert_eq!(ramp.triangulation, SurfaceDiagonal::NorthwestSoutheast);
    }

    #[test]
    fn steep_or_multi_level_surfaces_are_cliffs() {
        assert_eq!(
            from_heights([0, 100, 100, 0], Ratio::new(1, 1).expect("ratio")).kind,
            SurfaceKind::Cliff
        );
        assert_eq!(
            from_heights([0, 6_000, 6_000, 0], Ratio::new(30, 1).expect("ratio")).kind,
            SurfaceKind::Cliff
        );
    }
}
