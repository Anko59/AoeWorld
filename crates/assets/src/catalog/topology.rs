//! Reviewed sheet layout, independent of incidental source frame count.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainFrameTopology {
    /// Complete seamless grid; source index = x * rows + (-y mod rows).
    /// Preserve the reviewed row-zero world-coordinate phase.
    PeriodicXMajorReversedY { columns: u16, rows: u16 },
    /// Independent variants selected stably by world coordinate, not a grid.
    CoordinateStableAccents,
}

impl TerrainFrameTopology {
    /// Validate the selected range before loading or indexing a sheet.
    pub fn supports_frames(self, frames: u32) -> bool {
        match self {
            Self::PeriodicXMajorReversedY { columns, rows } => {
                columns > 0 && rows > 0 && frames == u32::from(columns) * u32::from(rows)
            }
            Self::CoordinateStableAccents => frames > 0,
        }
    }

    /// World-coordinate indexing for explicitly reviewed periodic sheets only.
    /// Reject incomplete/invalid grids and accents rather than guessing a layout.
    pub fn periodic_frame(self, x: i64, y: i64, frames: u32) -> Option<u32> {
        if !self.supports_frames(frames) {
            return None;
        }
        let Self::PeriodicXMajorReversedY { columns, rows } = self else {
            return None;
        };
        let x = x.rem_euclid(i64::from(columns)) as u32;
        let y = y.rem_euclid(i64::from(rows)) as u32;
        Some(x * u32::from(rows) + (u32::from(rows) - y) % u32::from(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_grid_order_wraps_both_axes_and_negative_coordinates() {
        let grid = TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        };
        assert_eq!(grid.periodic_frame(0, 0, 100), Some(0));
        assert_eq!(grid.periodic_frame(1, 0, 100), Some(10));
        assert_eq!(grid.periodic_frame(0, 1, 100), Some(9));
        assert_eq!(grid.periodic_frame(9, 9, 100), Some(91));
        assert_eq!(grid.periodic_frame(10, 10, 100), Some(0));
        assert_eq!(grid.periodic_frame(-1, -1, 100), Some(91));
        assert!(grid.periodic_frame(i64::MIN, i64::MAX, 100).unwrap() < 100);
    }

    #[test]
    fn frame_count_does_not_establish_periodicity() {
        let accents = TerrainFrameTopology::CoordinateStableAccents;
        assert!(accents.supports_frames(100));
        assert_eq!(accents.periodic_frame(0, 0, 100), None);
        assert!(!accents.supports_frames(0));
        let ice_sized_grid = TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 8,
            rows: 8,
        };
        assert!(ice_sized_grid.supports_frames(64));
        assert!(!ice_sized_grid.supports_frames(100));
        let invalid = TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 0,
            rows: 10,
        };
        assert_eq!(invalid.periodic_frame(0, 0, 0), None);
    }
}
