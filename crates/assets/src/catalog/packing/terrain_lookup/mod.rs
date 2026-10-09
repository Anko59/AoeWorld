//! Preparatory CPU-only terrain lookup; no runtime/shader integration.
//!
//! The atlas is exactly 48 MiB, page-major RGBA8. Page 2, row 1 owns 8192
//! bytes; row 0 (including the white texel) and every other byte are untouched.
//! All integers are little-endian. Header: bytes 0..4 `TLUT`, 4..6 version
//! (1), byte 6 descriptor stride in RGBA texels (3), byte 7 group count (7),
//! 8..10 total u16, 10..12 reserved zero, 12..16 layout checksum u32.
//! Seven 12-byte records at 16..100: base/count/columns/rows u16 at offsets
//! 0/2/4/6, kind u8 at 8 (0 missing, 1 periodic, 2 accents), flags u8 at 9,
//! reserved u16 at 10. Header padding 100..128 is zero. Descriptors start at
//! byte 128: x/y/width/height u16 at 0/2/4/6, page u8 at 8, flags u8 at 9,
//! reserved u16 at 10. Unused row bytes are zero. Thus capacity is 672, not
//! the packer's unchanged 2048 selected-frame limit. Dimensions stay raw.
//!
//! Checksum is FNV-1a-32 over the whole row, treating bytes 12..16 as zero.
//! It identifies layout bytes only: neither pixel/asset identity, catalog
//! approval, nor cryptographic authentication. Cached metadata retains no row.

use super::{ATLAS_BYTES, PAGE_BYTES, PAGE_SIDE, Placement};
use crate::catalog::TerrainFrameTopology;

mod constructed;
pub use constructed::ConstructedTerrainAtlas;
mod view;
pub use view::{ValidatedTable, validated_table};

pub const GROUP_COUNT: usize = 7;
pub const ROW_BYTES: usize = 8192;
pub const HEADER_BYTES: usize = 128;
pub const DESCRIPTOR_BYTES: usize = 12;
pub const MAX_ENTRIES: usize = (ROW_BYTES - HEADER_BYTES) / DESCRIPTOR_BYTES;
pub const ROW_OFFSET: usize = 2 * PAGE_BYTES + ROW_BYTES;

#[derive(Clone, Copy, Debug)]
pub struct TerrainGroup<'a> {
    pub placements: &'a [Placement],
    pub topology: Option<TerrainFrameTopology>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupMetadata {
    pub base: u16,
    pub count: u16,
    pub topology: Option<TerrainFrameTopology>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub groups: [GroupMetadata; GROUP_COUNT],
    pub total: u16,
    pub layout_checksum: u32,
}

impl Metadata {
    /// Returns a global descriptor index, with the authoritative signed phase.
    /// Accents use wrapping signed 32-bit 7*x + 13*y, then unsigned_abs.
    pub fn frame_index(&self, group: usize, x: i32, y: i32) -> Option<u16> {
        let group = self.groups.get(group)?;
        let topology = group.topology?;
        let frame = match topology {
            TerrainFrameTopology::PeriodicXMajorReversedY { .. } => {
                topology.periodic_frame(i64::from(x), i64::from(y), u32::from(group.count))?
            }
            TerrainFrameTopology::CoordinateStableAccents => {
                if group.count == 0 {
                    return None;
                }
                x.wrapping_mul(7)
                    .wrapping_add(y.wrapping_mul(13))
                    .unsigned_abs()
                    % u32::from(group.count)
            }
        };
        let index = group.base.checked_add(u16::try_from(frame).ok()?)?;
        (index < self.total).then_some(index)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExistingTable {
    /// Only the legacy all-zero reserved row may be written.
    Reject,
    /// Replace only a fully validated table owned by this version of the codec.
    ReplaceValidated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupError {
    AtlasLength,
    Capacity,
    Topology,
    Rectangle,
    Overlap,
    Header,
    Group,
    Reserved,
    Checksum,
    ExistingTable,
    Index,
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "terrain lookup error: {self:?}")
    }
}
impl std::error::Error for LookupError {}

/// Validate every input and existing row before the single atomic row copy.
/// Uses exactly one 8 KiB stack scratch row, no heap or retained table.
pub fn write_table(
    atlas: &mut [u8],
    groups: &[TerrainGroup<'_>; GROUP_COUNT],
    existing: ExistingTable,
) -> Result<Metadata, LookupError> {
    let old = decode(atlas)?;
    if old.is_some() && existing == ExistingTable::Reject {
        return Err(LookupError::ExistingTable);
    }
    write_validated_inputs(atlas, groups)
}

/// Construct a new table only in an exactly sized atlas with an all-zero row.
/// Unlike the general replacement API, any nonzero row is ExistingTable;
/// no untrusted existing table is decoded or accepted by this constructor.
/// Every input is validated before a single atomic copy, with one 8 KiB scratch.
pub fn write_new_table(
    atlas: &mut [u8],
    groups: &[TerrainGroup<'_>; GROUP_COUNT],
) -> Result<Metadata, LookupError> {
    if atlas.len() != ATLAS_BYTES {
        return Err(LookupError::AtlasLength);
    }
    if atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES]
        .iter()
        .any(|&byte| byte != 0)
    {
        return Err(LookupError::ExistingTable);
    }
    write_validated_inputs(atlas, groups)
}

// Both entry points establish row ownership and exact atlas size first. This
// helper validates inputs and constructs metadata without linking the decoder.
fn write_validated_inputs(
    atlas: &mut [u8],
    groups: &[TerrainGroup<'_>; GROUP_COUNT],
) -> Result<Metadata, LookupError> {
    let mut total = 0_usize;
    for group in groups {
        total = total
            .checked_add(group.placements.len())
            .ok_or(LookupError::Capacity)?;
        if total > MAX_ENTRIES {
            return Err(LookupError::Capacity);
        }
        validate_topology(group.topology, group.placements.len())?;
        for &placement in group.placements {
            validate_rectangle(placement)?;
        }
    }
    // Preserve flattened i/j order, borrowing prefixes instead of rescanning
    // seven groups to resolve both indices for every pair. No placement copy.
    for (group_index, group) in groups.iter().enumerate() {
        for (index, &placement) in group.placements.iter().enumerate() {
            for previous_group in &groups[..group_index] {
                for &previous in previous_group.placements {
                    if overlaps(placement, previous) {
                        return Err(LookupError::Overlap);
                    }
                }
            }
            for &previous in &group.placements[..index] {
                if overlaps(placement, previous) {
                    return Err(LookupError::Overlap);
                }
            }
        }
    }
    let mut row = [0_u8; ROW_BYTES];
    row[..4].copy_from_slice(b"TLUT");
    put16(&mut row, 4, 1);
    row[6] = 3;
    row[7] = 7;
    let total = u16::try_from(total).map_err(|_| LookupError::Capacity)?;
    put16(&mut row, 8, total);
    let mut metadata = Metadata {
        groups: [GroupMetadata {
            base: 0,
            count: 0,
            topology: None,
        }; GROUP_COUNT],
        total,
        layout_checksum: 0,
    };
    let mut base = 0_u16;
    for (index, group) in groups.iter().enumerate() {
        let count = u16::try_from(group.placements.len()).map_err(|_| LookupError::Capacity)?;
        metadata.groups[index] = GroupMetadata {
            base,
            count,
            topology: group.topology,
        };
        let offset = 16 + index * 12;
        put16(&mut row, offset, base);
        put16(&mut row, offset + 2, count);
        match group.topology {
            None => {}
            Some(TerrainFrameTopology::PeriodicXMajorReversedY { columns, rows }) => {
                put16(&mut row, offset + 4, columns);
                put16(&mut row, offset + 6, rows);
                row[offset + 8] = 1;
            }
            Some(TerrainFrameTopology::CoordinateStableAccents) => row[offset + 8] = 2,
        }
        for (local, &placement) in group.placements.iter().enumerate() {
            let offset = HEADER_BYTES + (usize::from(base) + local) * DESCRIPTOR_BYTES;
            put16(&mut row, offset, placement.x);
            put16(&mut row, offset + 2, placement.y);
            put16(&mut row, offset + 4, placement.width);
            put16(&mut row, offset + 6, placement.height);
            row[offset + 8] = u8::try_from(placement.page).map_err(|_| LookupError::Rectangle)?;
        }
        base = base.checked_add(count).ok_or(LookupError::Capacity)?;
    }
    let checksum = layout_checksum(&row);
    row[12..16].copy_from_slice(&checksum.to_le_bytes());
    metadata.layout_checksum = checksum;
    atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES].copy_from_slice(&row);
    Ok(metadata)
}

/// Legacy all-zero rows return None. All other malformed rows return an error.
/// Exact atlas length is required even when only the reserved row is read.
pub fn decode(atlas: &[u8]) -> Result<Option<Metadata>, LookupError> {
    if atlas.len() != ATLAS_BYTES {
        return Err(LookupError::AtlasLength);
    }
    let row = atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES]
        .try_into()
        .map_err(|_| LookupError::AtlasLength)?;
    decode_row(row)
}

/// Reads a descriptor only after validating the complete current layout.
/// A cached layout token is not a substitute for validating untrusted bytes.
pub fn descriptor(atlas: &[u8], index: u16) -> Result<Placement, LookupError> {
    validated_table(atlas)?
        .ok_or(LookupError::Index)?
        .descriptor(index)
}

fn decode_row(row: &[u8; ROW_BYTES]) -> Result<Option<Metadata>, LookupError> {
    if row.iter().all(|&byte| byte == 0) {
        return Ok(None);
    }
    if &row[..4] != b"TLUT" || get16(row, 4) != 1 || row[6] != 3 || row[7] != 7 {
        return Err(LookupError::Header);
    }
    if row[10..12].iter().chain(&row[100..128]).any(|&b| b != 0) {
        return Err(LookupError::Reserved);
    }
    let total = get16(row, 8);
    if usize::from(total) > MAX_ENTRIES {
        return Err(LookupError::Capacity);
    }
    let mut groups = [GroupMetadata {
        base: 0,
        count: 0,
        topology: None,
    }; GROUP_COUNT];
    let mut expected_base = 0_u16;
    for (index, group) in groups.iter_mut().enumerate() {
        let offset = 16 + index * 12;
        if row[offset + 9..offset + 12].iter().any(|&b| b != 0) {
            return Err(LookupError::Reserved);
        }
        let (base, count, columns, rows) = (
            get16(row, offset),
            get16(row, offset + 2),
            get16(row, offset + 4),
            get16(row, offset + 6),
        );
        let topology = match row[offset + 8] {
            0 if columns == 0 && rows == 0 => None,
            1 => Some(TerrainFrameTopology::PeriodicXMajorReversedY { columns, rows }),
            2 if columns == 0 && rows == 0 => Some(TerrainFrameTopology::CoordinateStableAccents),
            _ => return Err(LookupError::Group),
        };
        validate_topology(topology, usize::from(count))?;
        if base != expected_base {
            return Err(LookupError::Group);
        }
        expected_base = base.checked_add(count).ok_or(LookupError::Group)?;
        if expected_base > total {
            return Err(LookupError::Group);
        }
        *group = GroupMetadata {
            base,
            count,
            topology,
        };
    }
    if expected_base != total {
        return Err(LookupError::Group);
    }
    for index in 0..usize::from(total) {
        let offset = HEADER_BYTES + index * DESCRIPTOR_BYTES;
        if row[offset + 9..offset + 12].iter().any(|&b| b != 0) {
            return Err(LookupError::Reserved);
        }
        let placement = rectangle(row, index);
        validate_rectangle(placement)?;
        for previous in 0..index {
            if overlaps(placement, rectangle(row, previous)) {
                return Err(LookupError::Overlap);
            }
        }
    }
    if row[HEADER_BYTES + usize::from(total) * DESCRIPTOR_BYTES..]
        .iter()
        .any(|&b| b != 0)
    {
        return Err(LookupError::Reserved);
    }
    let checksum = u32::from_le_bytes([row[12], row[13], row[14], row[15]]);
    if checksum != layout_checksum(row) {
        return Err(LookupError::Checksum);
    }
    Ok(Some(Metadata {
        groups,
        total,
        layout_checksum: checksum,
    }))
}

fn validate_topology(
    topology: Option<TerrainFrameTopology>,
    count: usize,
) -> Result<(), LookupError> {
    match topology {
        None if count == 0 => Ok(()),
        Some(topology)
            if u32::try_from(count).is_ok_and(|count| topology.supports_frames(count)) =>
        {
            Ok(())
        }
        _ => Err(LookupError::Topology),
    }
}

fn validate_rectangle(p: Placement) -> Result<(), LookupError> {
    let side = u32::from(PAGE_SIDE);
    if p.page > 1
        || p.x < 2
        || p.y < 2
        || p.width == 0
        || p.height == 0
        || u32::from(p.x) + u32::from(p.width) + 1 >= side
        || u32::from(p.y) + u32::from(p.height) + 1 >= side
    {
        return Err(LookupError::Rectangle);
    }
    Ok(())
}

fn overlaps(a: Placement, b: Placement) -> bool {
    a.page == b.page
        && u32::from(a.x) <= u32::from(b.x) + u32::from(b.width)
        && u32::from(b.x) <= u32::from(a.x) + u32::from(a.width)
        && u32::from(a.y) <= u32::from(b.y) + u32::from(b.height)
        && u32::from(b.y) <= u32::from(a.y) + u32::from(a.height)
}

fn rectangle(row: &[u8], index: usize) -> Placement {
    let offset = HEADER_BYTES + index * DESCRIPTOR_BYTES;
    Placement {
        x: get16(row, offset),
        y: get16(row, offset + 2),
        width: get16(row, offset + 4),
        height: get16(row, offset + 6),
        page: u16::from(row[offset + 8]),
    }
}
fn get16(row: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([row[offset], row[offset + 1]])
}
fn put16(row: &mut [u8], offset: usize, value: u16) {
    row[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn layout_checksum(row: &[u8]) -> u32 {
    row.iter()
        .enumerate()
        .fold(2_166_136_261_u32, |hash, (index, &byte)| {
            let byte = if (12..16).contains(&index) { 0 } else { byte };
            (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
        })
}

#[cfg(test)]
mod tests;
