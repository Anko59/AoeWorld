//! Scene addresses keep page selection attached to UVs across every backend.

pub const GAME_ATLAS_SIDE: u32 = 2048;
pub const GAME_ATLAS_PAGES: u32 = 3;
pub const GAME_ATLAS_PAGE_BYTES: usize = GAME_ATLAS_SIDE as usize * GAME_ATLAS_SIDE as usize * 4;
pub const GAME_ATLAS_BYTES: usize = GAME_ATLAS_PAGE_BYTES * GAME_ATLAS_PAGES as usize;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasAddress {
    pub page: u32,
    /// Normalized origin and extent; an extent may be negative for mirroring.
    pub uv: [f32; 4],
}

impl AtlasAddress {
    /// Gameplay's explicitly reserved object-page texel. Diagnostics use page 0.
    pub const WHITE: Self = Self {
        page: 2,
        uv: [
            0.0,
            0.0,
            1.0 / GAME_ATLAS_SIDE as f32,
            1.0 / GAME_ATLAS_SIDE as f32,
        ],
    };

    pub const fn new(page: u32, uv: [f32; 4]) -> Self {
        Self { page, uv }
    }
}

/// Explicit scene metadata, supplied by the catalog adapter, never frame count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainTopology {
    PeriodicXMajorReversedY { columns: u16, rows: u16 },
    CoordinateStableAccents,
}

impl TerrainTopology {
    pub fn periodic_frame(self, x: i32, y: i32, frames: usize) -> Option<usize> {
        let Self::PeriodicXMajorReversedY { columns, rows } = self else {
            return None;
        };
        if columns == 0 || rows == 0 || usize::from(columns) * usize::from(rows) != frames {
            return None;
        }
        let row_count = i32::from(rows);
        let column = x.rem_euclid(i32::from(columns)) as usize;
        // Authored row zero stays zero: this is not rows - 1 - y.
        let row = (row_count - y.rem_euclid(row_count)).rem_euclid(row_count) as usize;
        Some(column * usize::from(rows) + row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn equal_uvs_on_distinct_pages_remain_distinct_addresses() {
        let uv = [0.0, 0.0, 0.25, 0.5];
        assert_ne!(AtlasAddress::new(0, uv), AtlasAddress::new(1, uv));
        assert_eq!(GAME_ATLAS_BYTES, 50_331_648);
        assert_eq!(AtlasAddress::WHITE.page, 2);
    }

    #[wasm_bindgen_test]
    fn declared_sheet_preserves_authored_signed_coordinate_phase() {
        let sheet = TerrainTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        };
        for (x, y, expected) in [(0, 0, 0), (0, 1, 9), (-1, -1, 91), (10, 10, 0)] {
            assert_eq!(sheet.periodic_frame(x, y, 100), Some(expected));
        }
        assert_eq!(sheet.periodic_frame(0, 0, 99), None);
        assert_eq!(
            TerrainTopology::CoordinateStableAccents.periodic_frame(0, 0, 100),
            None
        );
        assert_eq!(
            TerrainTopology::PeriodicXMajorReversedY {
                columns: 0,
                rows: 10
            }
            .periodic_frame(0, 0, 0),
            None
        );
    }
}

/// Rows of one RGBA page up to its last non-zero byte. Uploads stop there:
/// WebGPU and WebGL zero-initialize textures, and packed pages are mostly
/// empty, so trailing rows never need staging.
pub fn occupied_rows(page: &[u8]) -> u32 {
    let row = GAME_ATLAS_SIDE as usize * 4;
    page.chunks(row)
        .rposition(|bytes| bytes.iter().any(|&byte| byte != 0))
        .map_or(0, |last| last as u32 + 1)
}

#[cfg(test)]
#[path = "atlas/occupied_rows_tests.rs"]
mod occupied_rows_tests;
