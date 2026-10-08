use super::*;

#[test]
fn invalid_input_rectangles_and_unapproved_or_incomplete_topology_are_atomic() {
    let good = native(0, 0);
    let invalid = [
        Placement { width: 0, ..good },
        Placement { height: 0, ..good },
        Placement { page: 2, ..good },
        Placement {
            page: u16::MAX,
            ..good
        },
        Placement { x: 0, ..good },
        Placement { x: 1, ..good },
        Placement { y: 0, ..good },
        Placement { y: 1, ..good },
        Placement {
            width: 2045,
            ..good
        },
        Placement {
            height: 2045,
            ..good
        },
        Placement { x: 2047, ..good },
        Placement { y: 2047, ..good },
        Placement {
            x: u16::MAX,
            width: u16::MAX,
            ..good
        },
    ];
    let mut atlas = vec![0; ATLAS_BYTES];
    for p in invalid {
        let input = [good, p];
        let groups = one_group(&input, Some(ACCENTS));
        assert_eq!(
            write_table(&mut atlas, &groups, ExistingTable::Reject),
            Err(LookupError::Rectangle)
        );
        assert!(atlas.iter().all(|&b| b == 0));
    }
    let input = [good];
    for topology in [
        None,
        Some(GRID),
        Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 0,
            rows: 1,
        }),
    ] {
        let groups = one_group(&input, topology);
        assert_eq!(
            write_table(&mut atlas, &groups, ExistingTable::Reject),
            Err(LookupError::Topology)
        );
    }
    for topology in [Some(GRID), Some(ACCENTS)] {
        assert_eq!(
            write_table(&mut atlas, &one_group(&[], topology), ExistingTable::Reject),
            Err(LookupError::Topology)
        );
    }
    let overlap = [good, Placement { x: 3, ..good }];
    assert_eq!(
        write_table(
            &mut atlas,
            &one_group(&overlap, Some(ACCENTS)),
            ExistingTable::Reject
        ),
        Err(LookupError::Overlap)
    );
    assert!(atlas.iter().all(|&b| b == 0));
}

#[test]
fn atlas_length_legacy_zero_and_unknown_existing_rows_are_not_guessed() {
    assert_eq!(decode_row(&[0; ROW_BYTES]), Ok(None));
    let mut atlas = vec![0; ATLAS_BYTES + 1];
    assert_eq!(decode(&atlas), Err(LookupError::AtlasLength));
    for length in [0, 1, ROW_OFFSET, ATLAS_BYTES - 1] {
        assert_eq!(decode(&atlas[..length]), Err(LookupError::AtlasLength));
        assert_eq!(
            write_table(
                &mut atlas[..length],
                &[MISSING; 7],
                ExistingTable::ReplaceValidated
            ),
            Err(LookupError::AtlasLength)
        );
    }
    let atlas = &mut atlas[..ATLAS_BYTES];
    assert_eq!(decode(atlas), Ok(None));
    atlas[ROW_OFFSET + 8000] = 1;
    for policy in [ExistingTable::Reject, ExistingTable::ReplaceValidated] {
        assert_eq!(
            write_table(atlas, &[MISSING; 7], policy),
            Err(LookupError::Header)
        );
        assert_eq!(atlas[ROW_OFFSET + 8000], 1);
    }
    assert!(
        atlas[..ROW_OFFSET]
            .iter()
            .chain(&atlas[ROW_OFFSET + ROW_BYTES..])
            .all(|&b| b == 0)
    );
}

#[test]
fn malformed_header_groups_descriptors_reserved_and_checksum_fail_closed() {
    // Recompute checksums in structural mutations so schema validation is tested
    // independently of the checksum, including extreme untrusted u16 ranges.
    let original = valid_row();
    let cases: &[(usize, u8, LookupError)] = &[
        (0, b'X', LookupError::Header),
        (4, 2, LookupError::Header),
        (5, 1, LookupError::Header),
        (6, 12, LookupError::Header),
        (7, 8, LookupError::Header),
        (8, 255, LookupError::Group),
        (9, 255, LookupError::Capacity),
        (10, 1, LookupError::Reserved),
        (11, 1, LookupError::Reserved),
        (100, 1, LookupError::Reserved),
        (127, 1, LookupError::Reserved),
        (16, 1, LookupError::Group),
        (18, 0, LookupError::Topology),
        (19, 255, LookupError::Group),
        (20, 1, LookupError::Group),
        (22, 1, LookupError::Group),
        (24, 3, LookupError::Group),
        (24, 0, LookupError::Topology),
        (25, 1, LookupError::Reserved),
        (26, 1, LookupError::Reserved),
        (27, 1, LookupError::Reserved),
        (28, 0, LookupError::Group),
        (128, 1, LookupError::Rectangle),
        (130, 1, LookupError::Rectangle),
        (132, 0, LookupError::Rectangle),
        (134, 0, LookupError::Rectangle),
        (133, 255, LookupError::Rectangle),
        (136, 2, LookupError::Rectangle),
        (137, 1, LookupError::Reserved),
        (138, 1, LookupError::Reserved),
        (139, 1, LookupError::Reserved),
        (140, 1, LookupError::Reserved),
        (8191, 1, LookupError::Reserved),
    ];
    for &(offset, value, expected) in cases {
        let mut row = original;
        row[offset] = value;
        checksum_row(&mut row);
        assert_eq!(decode_row(&row), Err(expected), "offset {offset}");
    }
    for offset in [12, 13, 14, 15, 128, 136] {
        let mut row = original;
        row[offset] ^= 1; // valid x=3/page=0 layouts still require checksum
        assert_eq!(decode_row(&row), Err(LookupError::Checksum));
    }
    let mut row = original;
    row[24] = 1; // periodic, zero columns/rows
    checksum_row(&mut row);
    assert_eq!(decode_row(&row), Err(LookupError::Topology));
    put16(&mut row, 20, 2);
    put16(&mut row, 22, 3); // periodic product != count
    checksum_row(&mut row);
    assert_eq!(decode_row(&row), Err(LookupError::Topology));
}

#[test]
fn cross_group_overlap_and_missing_gutters_are_rejected_in_writer_and_decoder() {
    let first = [native(0, 0)];
    let second = [Placement {
        x: 99,
        ..native(0, 0)
    }];
    let mut groups = [MISSING; 7];
    groups[0] = TerrainGroup {
        placements: &first,
        topology: Some(ACCENTS),
    };
    groups[1] = TerrainGroup {
        placements: &second,
        topology: Some(ACCENTS),
    };
    let mut atlas = vec![0; ATLAS_BYTES];
    assert_eq!(
        write_table(&mut atlas, &groups, ExistingTable::Reject),
        Err(LookupError::Overlap)
    );
    assert!(atlas.iter().all(|&b| b == 0));
    let mut row = valid_row();
    put16(&mut row, 8, 2);
    put16(&mut row, 18, 2);
    for index in 1..7 {
        put16(&mut row, 16 + index * 12, 2);
    }
    let second = [99, 0, 2, 0, 97, 0, 49, 0, 1, 0, 0, 0];
    row[140..152].copy_from_slice(&second);
    checksum_row(&mut row);
    assert_eq!(decode_row(&row), Err(LookupError::Overlap));
    put16(&mut row, 140, 100); // one-pixel gutter is valid
    checksum_row(&mut row);
    assert!(decode_row(&row).unwrap().is_some());
}

#[test]
fn corrupt_owned_table_cannot_be_replaced_even_with_explicit_permission() {
    let mut atlas = vec![0; ATLAS_BYTES];
    let row = valid_row();
    atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES].copy_from_slice(&row);
    atlas[ROW_OFFSET + 12] ^= 1;
    assert_eq!(
        write_table(&mut atlas, &[MISSING; 7], ExistingTable::ReplaceValidated),
        Err(LookupError::Checksum)
    );
    assert_eq!(atlas[ROW_OFFSET + 12], row[12] ^ 1);
}

#[test]
fn real_packer_maximum_boundary_preserves_outputs_and_all_placements_start_at_two() {
    let frames = [
        FrameExtent {
            domain: AtlasDomain::Terrain,
            width: 2044,
            height: 2044,
        },
        FrameExtent {
            domain: AtlasDomain::Terrain,
            width: 2044,
            height: 2044,
        },
        FrameExtent {
            domain: AtlasDomain::Objects,
            width: 2044,
            height: 2044,
        },
    ];
    let placements = pack_frames(&frames).unwrap();
    for (index, p) in placements.iter().enumerate() {
        assert_eq!(
            *p,
            Placement {
                page: u16::try_from(index).unwrap(),
                x: 2,
                y: 2,
                width: 2044,
                height: 2044
            }
        );
    }
    let groups = one_group(&placements[..2], Some(ACCENTS));
    let mut atlas = vec![0; ATLAS_BYTES];
    let metadata = write_table(&mut atlas, &groups, ExistingTable::Reject).unwrap();
    assert_eq!(metadata.total, 2);
    assert_eq!(descriptor(&atlas, 1), Ok(placements[1]));
    let small = [FrameExtent {
        domain: AtlasDomain::Terrain,
        width: 1,
        height: 1,
    }; MAX_SELECTED_FRAMES];
    let packed = pack_frames(&small).unwrap();
    assert_eq!(packed.len(), 2048);
    assert!(packed.iter().all(|p| p.y >= 2 && p.x >= 2));
    assert_eq!((packed[0].x, packed[0].y), (2, 2));
    assert_eq!((packed[1021].x, packed[1021].y), (2044, 2));
    assert_eq!((packed[1022].x, packed[1022].y), (2, 4));
    assert_eq!((packed[2047].x, packed[2047].y), (8, 6));
}
