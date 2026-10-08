use super::*;
use crate::catalog::packing::{
    AtlasDomain, FrameExtent, MAX_SELECTED_FRAMES, WHITE_TEXEL, pack_frames,
};

const MISSING: TerrainGroup<'static> = TerrainGroup {
    placements: &[],
    topology: None,
};
const ACCENTS: TerrainFrameTopology = TerrainFrameTopology::CoordinateStableAccents;
const GRID: TerrainFrameTopology = TerrainFrameTopology::PeriodicXMajorReversedY {
    columns: 2,
    rows: 3,
};

fn native(index: u16, page: u16) -> Placement {
    Placement {
        page,
        x: 2 + index * 98,
        y: 2,
        width: 97,
        height: 49,
    }
}
fn one_group(
    placements: &[Placement],
    topology: Option<TerrainFrameTopology>,
) -> [TerrainGroup<'_>; GROUP_COUNT] {
    let mut groups = [MISSING; GROUP_COUNT];
    groups[0] = TerrainGroup {
        placements,
        topology,
    };
    groups
}
fn checksum_row(row: &mut [u8]) {
    let checksum = layout_checksum(row);
    row[12..16].copy_from_slice(&checksum.to_le_bytes());
}
fn valid_row() -> [u8; ROW_BYTES] {
    let mut row = [0; ROW_BYTES];
    row[..4].copy_from_slice(b"TLUT");
    put16(&mut row, 4, 1);
    row[6] = 3;
    row[7] = 7;
    put16(&mut row, 8, 1);
    put16(&mut row, 18, 1);
    row[24] = 2;
    for index in 1..GROUP_COUNT {
        put16(&mut row, 16 + index * 12, 1);
    }
    row[128..140].copy_from_slice(&[2, 0, 2, 0, 97, 0, 49, 0, 1, 0, 0, 0]);
    checksum_row(&mut row);
    assert!(decode_row(&row).unwrap().is_some());
    row
}

mod new_table;
mod roundtrip;
mod validation;
mod view;
