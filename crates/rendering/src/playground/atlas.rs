//! Scene addresses keep page selection attached to UVs across every backend.

pub const GAME_ATLAS_SIDE: u32 = 2048;
pub const GAME_ATLAS_PAGES: u32 = 8;
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
    Dds {
        size: [u16; 2],
        period: [u16; 2],
        level: u8,
    },
    PeriodicXMajorReversedY {
        columns: u16,
        rows: u16,
    },
    CoordinateStableAccents,
}

#[path = "atlas/dds.rs"]
mod dds;

impl TerrainTopology {
    pub fn periodic_frame(self, x: i32, y: i32, frames: usize) -> Option<usize> {
        if let Some((index, _, _)) = self.planar([f64::from(x), f64::from(y)]) {
            return (index < frames).then_some(index);
        }
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

#[path = "atlas/tests.rs"]
#[cfg(test)]
mod tests;

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

/// Bounded texture-array growth. Pages 0–7 are the base; further object
/// colour/mask pairs are allocated only when the selected art needs them.
pub const MAX_GAME_ATLAS_PAGES: u32 = 64;
pub fn atlas_pages(bytes: usize) -> Option<u32> {
    let pages = bytes / GAME_ATLAS_PAGE_BYTES;
    (bytes.is_multiple_of(GAME_ATLAS_PAGE_BYTES)
        && (GAME_ATLAS_PAGES as usize..=MAX_GAME_ATLAS_PAGES as usize).contains(&pages)
        && pages.is_multiple_of(2))
    .then_some(pages as u32)
}
pub const fn atlas_mask_page(colour: u32) -> u32 {
    if colour < 5 { colour + 3 } else { colour + 1 }
}
