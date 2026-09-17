//! Classic DRS table parser. Offsets are checked before slices or allocations.
use crate::{Error, invalid, slice, u32_at};
use std::collections::BTreeSet;

const FORMAT: &str = "DRS";
const MAX_ARCHIVE: usize = 512 * 1024 * 1024;
const MAX_FILES: usize = 100_000;

#[derive(Clone, Debug)]
pub struct Entry<'a> {
    pub kind: [u8; 4],
    pub id: u32,
    pub data: &'a [u8],
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Entry<'_>>, Error> {
    if bytes.len() > MAX_ARCHIVE {
        return Err(invalid(FORMAT, 0, "archive exceeds 512 MiB"));
    }
    let version = slice(bytes, 40, 4, FORMAT)?;
    if version != b"1.00" {
        return Err(Error::Unsupported {
            format: FORMAT,
            detail: format!("version {:?}", version),
        });
    }
    let table_count = usize::try_from(u32_at(bytes, 56, FORMAT)?)
        .map_err(|_| invalid(FORMAT, 56, "table count overflow"))?;
    if table_count > 32 {
        return Err(invalid(FORMAT, 56, "too many tables"));
    }
    let first_data = u32_at(bytes, 60, FORMAT)? as usize;
    if first_data > bytes.len() {
        return Err(invalid(FORMAT, 60, "first file offset beyond archive"));
    }
    let mut entries = Vec::new();
    let mut identifiers = BTreeSet::new();
    for table in 0..table_count {
        let offset = 64usize
            .checked_add(
                table
                    .checked_mul(12)
                    .ok_or_else(|| invalid(FORMAT, 64, "table overflow"))?,
            )
            .ok_or_else(|| invalid(FORMAT, 64, "table overflow"))?;
        let kind: [u8; 4] = slice(bytes, offset, 4, FORMAT)?
            .try_into()
            .map_err(|_| invalid(FORMAT, offset, "kind"))?;
        let records = u32_at(bytes, offset + 4, FORMAT)? as usize;
        let count = u32_at(bytes, offset + 8, FORMAT)? as usize;
        if count > MAX_FILES || entries.len().saturating_add(count) > MAX_FILES {
            return Err(invalid(FORMAT, offset + 8, "too many files"));
        }
        slice(
            bytes,
            records,
            count
                .checked_mul(12)
                .ok_or_else(|| invalid(FORMAT, records, "record size overflow"))?,
            FORMAT,
        )?;
        for index in 0..count {
            let record = records + index * 12;
            let id = u32_at(bytes, record, FORMAT)?;
            let data_offset = u32_at(bytes, record + 4, FORMAT)? as usize;
            let len = u32_at(bytes, record + 8, FORMAT)? as usize;
            if data_offset < first_data {
                return Err(invalid(FORMAT, record + 4, "file overlaps headers"));
            }
            let data = slice(bytes, data_offset, len, FORMAT)?;
            if !identifiers.insert((kind, id)) {
                return Err(invalid(FORMAT, record, "duplicate resource identifier"));
            }
            entries.push(Entry { kind, id, data });
        }
    }
    entries.sort_by_key(|entry| (entry.kind, entry.id));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0u8; 95];
        bytes[40..44].copy_from_slice(b"1.00");
        bytes[56..60].copy_from_slice(&1u32.to_le_bytes());
        bytes[60..64].copy_from_slice(&92u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b" pls");
        bytes[68..72].copy_from_slice(&76u32.to_le_bytes());
        bytes[72..76].copy_from_slice(&1u32.to_le_bytes());
        bytes[76..80].copy_from_slice(&50500u32.to_le_bytes());
        bytes[80..84].copy_from_slice(&92u32.to_le_bytes());
        bytes[84..88].copy_from_slice(&3u32.to_le_bytes());
        bytes[92..95].copy_from_slice(b"abc");
        bytes
    }

    #[test]
    fn decodes_fixture_and_rejects_bounds() {
        let data = fixture();
        let entries = parse(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 50500);
        assert_eq!(entries[0].data, b"abc");
        assert!(parse(&data[..94]).is_err());
        let mut corrupt = data;
        corrupt[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&corrupt).is_err());
    }

    #[test]
    fn rejects_invalid_tables_offsets_and_resource_ids() {
        let original = fixture();
        assert!(parse(&original[..40]).is_err());
        for (range, value) in [
            (40..44, 0u32),
            (56..60, 33u32),
            (60..64, 96u32),
            (68..72, u32::MAX),
            (72..76, 100_001u32),
            (80..84, 91u32),
        ] {
            let mut bytes = original.clone();
            bytes[range].copy_from_slice(&value.to_le_bytes());
            assert!(parse(&bytes).is_err());
        }
        let mut duplicate = original;
        duplicate.resize(107, 0);
        duplicate[60..64].copy_from_slice(&104u32.to_le_bytes());
        duplicate[72..76].copy_from_slice(&2u32.to_le_bytes());
        duplicate[80..84].copy_from_slice(&104u32.to_le_bytes());
        duplicate[88..92].copy_from_slice(&50500u32.to_le_bytes());
        duplicate[92..96].copy_from_slice(&104u32.to_le_bytes());
        duplicate[96..100].copy_from_slice(&0u32.to_le_bytes());
        duplicate[104..107].copy_from_slice(b"abc");
        assert!(parse(&duplicate).is_err());
    }
}
