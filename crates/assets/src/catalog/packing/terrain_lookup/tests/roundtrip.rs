use super::*;

#[test]
fn roundtrip_seven_explicit_groups_raw_native_frames_and_both_pages() {
    let grid: [Placement; 6] = std::array::from_fn(|i| native(u16::try_from(i).unwrap(), 0));
    let accents: [Placement; 10] = std::array::from_fn(|i| native(u16::try_from(i).unwrap(), 1));
    let single: [Placement; 5] = std::array::from_fn(|i| Placement {
        y: 60 + u16::try_from(i).unwrap() * 50,
        ..native(0, 0)
    });
    let groups = [
        TerrainGroup {
            placements: &grid,
            topology: Some(GRID),
        },
        TerrainGroup {
            placements: &accents,
            topology: Some(ACCENTS),
        },
        TerrainGroup {
            placements: &single[..1],
            topology: Some(ACCENTS),
        },
        TerrainGroup {
            placements: &single[1..2],
            topology: Some(ACCENTS),
        },
        TerrainGroup {
            placements: &single[2..3],
            topology: Some(ACCENTS),
        },
        TerrainGroup {
            placements: &single[3..4],
            topology: Some(ACCENTS),
        },
        TerrainGroup {
            placements: &single[4..],
            topology: Some(ACCENTS),
        },
    ];
    let mut atlas = vec![0xA5; ATLAS_BYTES];
    atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES].fill(0);
    let white = usize::from(WHITE_TEXEL.page) * PAGE_BYTES;
    atlas[white..white + 4].fill(255);
    let metadata = write_table(&mut atlas, &groups, ExistingTable::Reject).unwrap();
    assert_eq!(metadata.total, 21);
    assert_eq!(decode(&atlas), Ok(Some(metadata)));
    assert_eq!(
        &atlas[ROW_OFFSET..ROW_OFFSET + 12],
        &[84, 76, 85, 84, 1, 0, 3, 7, 21, 0, 0, 0]
    );
    assert_eq!(
        &atlas[ROW_OFFSET + 16..ROW_OFFSET + 28],
        &[0, 0, 6, 0, 2, 0, 3, 0, 1, 0, 0, 0]
    );
    assert_eq!(
        &atlas[ROW_OFFSET + 128..ROW_OFFSET + 140],
        &[2, 0, 2, 0, 97, 0, 49, 0, 0, 0, 0, 0]
    );
    let mut expected_base = 0;
    for (group_index, input) in groups.iter().enumerate() {
        let cached = metadata.groups[group_index];
        assert_eq!(cached.base, expected_base);
        assert_eq!(usize::from(cached.count), input.placements.len());
        assert_eq!(cached.topology, input.topology);
        for &expected in input.placements {
            assert_eq!(descriptor(&atlas, expected_base), Ok(expected));
            expected_base += 1;
        }
    }
    assert_eq!(descriptor(&atlas, 21), Err(LookupError::Index));
    assert_eq!(&atlas[white..white + 4], &[255; 4]);
    // Check ALL bytes outside row, not merely representative addresses.
    for (index, &byte) in atlas.iter().enumerate() {
        if (ROW_OFFSET..ROW_OFFSET + ROW_BYTES).contains(&index) {
            continue;
        }
        assert_eq!(
            byte,
            if (white..white + 4).contains(&index) {
                255
            } else {
                0xA5
            }
        );
    }
    let before = metadata;
    assert_eq!(
        write_table(&mut atlas, &groups, ExistingTable::Reject),
        Err(LookupError::ExistingTable)
    );
    assert_eq!(decode(&atlas), Ok(Some(before)));
    let empty = [MISSING; GROUP_COUNT];
    let replacement = write_table(&mut atlas, &empty, ExistingTable::ReplaceValidated).unwrap();
    assert_eq!(replacement.total, 0);
    assert_eq!(decode(&atlas), Ok(Some(replacement)));
}

#[test]
fn signed_asymmetric_periodic_phase_and_wrapping_accent_hash_are_exact() {
    let groups = [GroupMetadata {
        base: 10,
        count: 6,
        topology: Some(GRID),
    }; GROUP_COUNT];
    let mut metadata = Metadata {
        groups,
        total: 30,
        layout_checksum: 0,
    };
    for (x, y, expected) in [
        (0, 0, 10),
        (1, 0, 13),
        (0, 1, 12),
        (0, 2, 11),
        (-1, -1, 14),
        (2, 3, 10),
        (i32::MIN, i32::MAX, 12),
        (i32::MAX, i32::MIN, 15),
    ] {
        assert_eq!(metadata.frame_index(0, x, y), Some(expected));
    }
    metadata.groups[1] = GroupMetadata {
        base: 20,
        count: 10,
        topology: Some(ACCENTS),
    };
    // The MIN cases exercise unsigned_abs rather than overflowing signed abs.
    for (x, y, expected) in [
        (0, 0, 20),
        (1, 0, 27),
        (0, 1, 23),
        (-1, -1, 20),
        (i32::MIN, 0, 28),
        (0, i32::MIN, 28),
        (i32::MAX, i32::MAX, 20),
        (i32::MIN, i32::MAX, 23),
    ] {
        assert_eq!(metadata.frame_index(1, x, y), Some(expected));
    }
    metadata.groups[2] = GroupMetadata {
        base: 30,
        count: 0,
        topology: None,
    };
    assert_eq!(metadata.frame_index(2, 0, 0), None);
    assert_eq!(metadata.frame_index(7, 0, 0), None);
}

#[test]
fn exact_header_and_golden_descriptor_bytes() {
    assert_eq!(layout_checksum(b""), 0x811c_9dc5);
    assert_eq!(layout_checksum(b"a"), 0xe40c_292c);
    assert_eq!(layout_checksum(b"foobar"), 0xbf9c_f968);
    let row = valid_row();
    assert_eq!(&row[..12], &[84, 76, 85, 84, 1, 0, 3, 7, 1, 0, 0, 0]);
    assert_eq!(&row[16..28], &[0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(&row[28..40], &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&row[100..128], &[0; 28]);
    assert_eq!(&row[128..140], &[2, 0, 2, 0, 97, 0, 49, 0, 1, 0, 0, 0]);
    assert!(row[140..].iter().all(|&b| b == 0));
    let metadata = decode_row(&row).unwrap().unwrap();
    assert_eq!(&row[12..16], &metadata.layout_checksum.to_le_bytes());
}

#[test]
fn capacity_672_is_row_only_and_673_is_atomic_with_fixed_borrowed_inputs() {
    assert_eq!(MAX_ENTRIES, 672);
    assert_eq!(MAX_SELECTED_FRAMES, 2048);
    // Fixed input arrays; the codec itself has no allocation interface or Vec.
    let placements: [Placement; 673] = std::array::from_fn(|i| Placement {
        page: 0,
        x: 2 + 2 * u16::try_from(i).unwrap(),
        y: 2,
        width: 1,
        height: 1,
    });
    let mut atlas = vec![0; ATLAS_BYTES];
    let groups = one_group(&placements[..672], Some(ACCENTS));
    let metadata = write_table(&mut atlas, &groups, ExistingTable::Reject).unwrap();
    assert_eq!(metadata.total, 672);
    assert_eq!(descriptor(&atlas, 671), Ok(placements[671]));
    assert!(
        atlas[ROW_OFFSET + 8192 - 4..ROW_OFFSET + 8192]
            .iter()
            .all(|&b| b == 0)
    );
    let before: [u8; ROW_BYTES] = atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES]
        .try_into()
        .unwrap();
    let excess = one_group(&placements, Some(ACCENTS));
    assert_eq!(
        write_table(&mut atlas, &excess, ExistingTable::ReplaceValidated),
        Err(LookupError::Capacity)
    );
    assert_eq!(&atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES], &before);
    assert!(
        atlas[..ROW_OFFSET]
            .iter()
            .chain(&atlas[ROW_OFFSET + ROW_BYTES..])
            .all(|&b| b == 0)
    );
}
