//! Frozen flattened writer versus borrowed-prefix traversal, including failures.
use super::*;

fn frozen(
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
    // Independent frozen old flattened i/j loop; do not call the new traversal.
    let placement_at = |index: usize| {
        let mut index = index;
        for group in groups {
            if index < group.placements.len() {
                return group.placements[index];
            }
            index -= group.placements.len();
        }
        unreachable!("validated bounded flattened index")
    };
    for i in 0..total {
        for j in 0..i {
            if overlaps(placement_at(i), placement_at(j)) {
                return Err(LookupError::Overlap);
            }
        }
    }
    let mut row = [0_u8; ROW_BYTES];
    row[..4].copy_from_slice(b"TLUT");
    put16(&mut row, 4, 1);
    row[6] = 3;
    row[7] = 7;
    put16(&mut row, 8, total as u16);
    let mut base = 0_u16;
    for (index, group) in groups.iter().enumerate() {
        let count = group.placements.len() as u16;
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
        for (local, &p) in group.placements.iter().enumerate() {
            let offset = HEADER_BYTES + (usize::from(base) + local) * DESCRIPTOR_BYTES;
            put16(&mut row, offset, p.x);
            put16(&mut row, offset + 2, p.y);
            put16(&mut row, offset + 4, p.width);
            put16(&mut row, offset + 6, p.height);
            row[offset + 8] = p.page as u8;
        }
        base += count;
    }
    checksum_row(&mut row);
    atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES].copy_from_slice(&row);
    decode(atlas)?.ok_or(LookupError::Header)
}

fn placements(counts: [usize; GROUP_COUNT]) -> [Vec<Placement>; GROUP_COUNT] {
    let mut index = 0_usize;
    counts.map(|count| {
        (0..count)
            .map(|_| {
                let mut p = native((index % 20) as u16, (index % 2) as u16);
                p.y = 2 + (index / 20) as u16 * 50;
                index += 1;
                p
            })
            .collect()
    })
}

fn groups(data: &[Vec<Placement>; GROUP_COUNT]) -> [TerrainGroup<'_>; GROUP_COUNT] {
    std::array::from_fn(|slot| TerrainGroup {
        placements: &data[slot],
        topology: (!data[slot].is_empty()).then_some(ACCENTS),
    })
}

fn compare(
    old: &mut [u8],
    new: &mut [u8],
    input: &[TerrainGroup<'_>; GROUP_COUNT],
    expected: Option<LookupError>,
) {
    let range = ROW_OFFSET..ROW_OFFSET + ROW_BYTES;
    old[range.clone()].fill(0);
    new[range.clone()].fill(0);
    let reference = frozen(old, input);
    if let Some(error) = expected {
        assert_eq!(reference, Err(error));
    } else {
        assert!(reference.is_ok(), "unexpected frozen error: {reference:?}");
    }
    assert_eq!(write_new_table(new, input), reference);
    assert!(
        old == new,
        "every atlas byte must equal the independent frozen writer"
    );
    if reference.is_err() {
        assert!(
            new[range.clone()].iter().all(|&byte| byte == 0),
            "failure mutated owned row"
        );
    }
    // General writer shares the same traversal but retains its old admission.
    new[range].fill(0);
    assert_eq!(write_table(new, input, ExistingTable::Reject), reference);
    assert!(
        old == new,
        "general writer differs from independent frozen writer"
    );
}

#[test]
fn borrowed_prefix_overlap_matches_frozen_many_empty_groups_and_native_pages() {
    let mut old = vec![0xA5; ATLAS_BYTES];
    let mut new = old.clone();
    let empty = placements([0; GROUP_COUNT]);
    compare(&mut old, &mut new, &groups(&empty), None);
    for seed in 0..12 {
        let counts = std::array::from_fn(|slot| (seed + slot * 3) % 5);
        let data = placements(counts);
        compare(&mut old, &mut new, &groups(&data), None);
    }
    for slot in 0..GROUP_COUNT {
        let mut counts = [0; GROUP_COUNT];
        counts[slot] = 6;
        let data = placements(counts);
        let mut input = groups(&data);
        input[slot].topology = Some(GRID);
        compare(&mut old, &mut new, &input, None);
    }
    let data = placements([6, 0, 10, 0, 0, 0, 4]);
    let mut input = groups(&data);
    input[0].topology = Some(GRID);
    compare(&mut old, &mut new, &input, None);
    assert!(old[..ROW_OFFSET].iter().all(|&byte| byte == 0xA5));
    assert!(
        old[ROW_OFFSET + ROW_BYTES..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );
}

#[test]
fn borrowed_prefix_retains_first_last_pairs_gutter_boundaries_and_error_precedence() {
    let mut old = vec![0xA5; ATLAS_BYTES];
    let mut new = old.clone();
    for variant in 0..5 {
        let mut data = placements([2, 2, 0, 3, 0, 2, 2]);
        match variant {
            0 => data[0][1] = data[0][0],
            1 => data[3][1] = data[0][1],
            2 => data[6][1] = data[0][0],
            3 => data[6][1] = data[6][0],
            _ => {
                data[6][1] = Placement {
                    page: data[0][0].page ^ 1,
                    ..data[0][0]
                }
            }
        }
        compare(
            &mut old,
            &mut new,
            &groups(&data),
            (variant < 4).then_some(LookupError::Overlap),
        );
    }
    for distance in [97, 98, 99] {
        let mut data = placements([1, 0, 0, 0, 0, 0, 1]);
        data[6][0] = Placement {
            x: data[0][0].x + distance,
            ..data[0][0]
        };
        compare(
            &mut old,
            &mut new,
            &groups(&data),
            (distance == 97).then_some(LookupError::Overlap),
        );
    }
    let mut data = placements([2, 0, 0, 0, 0, 0, 1]);
    data[0][1] = data[0][0];
    data[6][0].width = 0;
    compare(
        &mut old,
        &mut new,
        &groups(&data),
        Some(LookupError::Rectangle),
    );
    data[6][0].width = 97;
    let mut input = groups(&data);
    input[6].topology = None;
    compare(&mut old, &mut new, &input, Some(LookupError::Topology));
    let oversized = placements([MAX_ENTRIES + 1, 0, 0, 0, 0, 0, 0]);
    compare(
        &mut old,
        &mut new,
        &groups(&oversized),
        Some(LookupError::Capacity),
    );
}
