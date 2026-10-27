use super::*;
use aoe_map::NavigationStorage;
use memmap2::Mmap;
use std::ops::Range;

#[derive(Debug)]
struct Mapping(Mmap);
impl NavigationStorage for Mapping {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
    fn heap_bytes(&self) -> usize {
        size_of::<Self>()
    }
}
fn invalid() -> String {
    "invalid navigation index or record".to_owned()
}

pub(super) fn read(
    path: &Path,
    hash: [u8; 32],
    side: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Arc<NavigationSnapshot>, String> {
    let count = side.checked_mul(side).ok_or_else(invalid)?;
    if count == 0 || count > MAX_INDEX_HEAP_BYTES / size_of::<Range<usize>>() {
        return Err(invalid());
    }
    let index_end = HEADER_BYTES + count * INDEX_RECORD_BYTES;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length < index_end as u64 || length > MAX_FILE_BYTES {
        return Err("navigation storage budget or length differs".into());
    }
    let mapping = Mapping(super::immutable::map(&mut file, length, cancelled)?);
    let bytes = mapping.bytes();
    if &bytes[..8] != MAGIC || bytes[8..40] != hash || bytes[40..44] != (side as u32).to_le_bytes()
    {
        return Err("navigation package identity or dimensions differ".into());
    }
    let mut records = Vec::with_capacity(count);
    for at in 0..count {
        if cancelled() {
            return Err("navigation load cancelled".into());
        }
        let entry = HEADER_BYTES + at * INDEX_RECORD_BYTES;
        let start = u64::from_le_bytes(bytes[entry..entry + 8].try_into().map_err(|_| invalid())?);
        let length = u32::from_le_bytes(
            bytes[entry + 8..entry + 12]
                .try_into()
                .map_err(|_| invalid())?,
        ) as usize;
        let start = usize::try_from(start).map_err(|_| invalid())?;
        let end = start.checked_add(length).ok_or_else(invalid)?;
        if start < index_end || length > MAX_CHUNK_BYTES {
            return Err(invalid());
        }
        let chunk = bytes.get(start..end).ok_or_else(invalid)?;
        if aoe_map::navigation_checksum(chunk) != bytes[entry + 12..entry + 44] {
            return Err("navigation chunk checksum differs".into());
        }
        records.push(start..end);
    }
    // Completion order on disk can differ from chunk order in the index.
    // Reject overlap, holes, duplicate payloads and trailing bytes before use.
    let mut ordered = records.clone();
    ordered.sort_unstable_by_key(|range| range.start);
    let mut end = index_end;
    for range in ordered {
        if range.start != end {
            return Err(invalid());
        }
        end = range.end;
    }
    if end != bytes.len() {
        return Err("navigation has trailing bytes".into());
    }
    NavigationSnapshot::from_storage(side as i32, Arc::new(mapping), records, cancelled)
        .map(Arc::new)
        .map_err(|e| e.to_string())
}
