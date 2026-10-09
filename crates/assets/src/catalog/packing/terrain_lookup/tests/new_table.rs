use super::*;

fn equivalent(groups: &[TerrainGroup<'_>; GROUP_COUNT]) {
    let mut original = vec![0xA5; ATLAS_BYTES];
    original[ROW_OFFSET..ROW_OFFSET + ROW_BYTES].fill(0);
    let white = usize::from(WHITE_TEXEL.page) * PAGE_BYTES;
    original[white..white + 4].fill(255);
    let mut fresh = original.clone();
    let general = write_table(&mut original, groups, ExistingTable::Reject).unwrap();
    let constructed = write_new_table(&mut fresh, groups).unwrap();
    assert_eq!(constructed, general);
    assert_eq!(decode(&fresh), Ok(Some(constructed)));
    assert!(fresh == original, "entire atlas must match general writer");
    assert_eq!(&fresh[white..white + 4], &[255; 4]);
    for (index, &byte) in fresh.iter().enumerate() {
        if !(ROW_OFFSET..ROW_OFFSET + ROW_BYTES).contains(&index) {
            assert_eq!(
                byte,
                if (white..white + 4).contains(&index) {
                    255
                } else {
                    0xA5
                }
            );
        }
    }
}

#[test]
fn new_constructor_matches_general_writer_empty_native_pages_and_510_frames() {
    equivalent(&[MISSING; GROUP_COUNT]);
    let periodic = std::array::from_fn::<_, 6, _>(|i| native(i as u16, 0));
    let accents = std::array::from_fn::<_, 10, _>(|i| native(i as u16, 1));
    let mut small = one_group(&periodic, Some(GRID));
    small[6] = TerrainGroup {
        placements: &accents,
        topology: Some(ACCENTS),
    };
    equivalent(&small);
    let extents = vec![
        FrameExtent {
            width: 97,
            height: 49,
            domain: AtlasDomain::Terrain
        };
        510
    ];
    let placements = pack_frames(&extents).unwrap();
    let topology = Some(TerrainFrameTopology::PeriodicXMajorReversedY {
        columns: 10,
        rows: 10,
    });
    let groups = [
        TerrainGroup {
            placements: &placements[0..100],
            topology,
        },
        TerrainGroup {
            placements: &placements[100..200],
            topology,
        },
        TerrainGroup {
            placements: &placements[200..300],
            topology,
        },
        TerrainGroup {
            placements: &placements[300..400],
            topology,
        },
        MISSING,
        TerrainGroup {
            placements: &placements[400..500],
            topology,
        },
        TerrainGroup {
            placements: &placements[500..510],
            topology: Some(ACCENTS),
        },
    ];
    equivalent(&groups);
}

#[test]
fn new_constructor_nonzero_row_rejection_is_atomic_and_general_errors_stay_distinct() {
    let mut atlas = vec![0xA5; ATLAS_BYTES];
    let row = ROW_OFFSET..ROW_OFFSET + ROW_BYTES;
    atlas[row.clone()].fill(0);
    for offset in [0, 4, 7, 10, 12, 16, 99, 100, 127, 128, 139, ROW_BYTES - 1] {
        atlas[ROW_OFFSET + offset] = 1;
        let before = atlas[row.clone()].to_vec();
        assert_eq!(
            write_new_table(&mut atlas, &[MISSING; GROUP_COUNT]),
            Err(LookupError::ExistingTable)
        );
        assert_eq!(&atlas[row.clone()], &before);
        atlas[ROW_OFFSET + offset] = 0;
    }
    atlas[row.clone()].copy_from_slice(&valid_row());
    let before = atlas[row.clone()].to_vec();
    assert_eq!(
        write_new_table(&mut atlas, &[MISSING; GROUP_COUNT]),
        Err(LookupError::ExistingTable)
    );
    assert_eq!(
        write_table(&mut atlas, &[MISSING; GROUP_COUNT], ExistingTable::Reject),
        Err(LookupError::ExistingTable)
    );
    assert_eq!(&atlas[row.clone()], &before);
    atlas[ROW_OFFSET + 12] ^= 1;
    let before = atlas[row.clone()].to_vec();
    assert_eq!(
        write_new_table(&mut atlas, &[MISSING; GROUP_COUNT]),
        Err(LookupError::ExistingTable)
    );
    assert_eq!(
        write_table(&mut atlas, &[MISSING; GROUP_COUNT], ExistingTable::Reject),
        Err(LookupError::Checksum)
    );
    assert_eq!(
        write_table(
            &mut atlas,
            &[MISSING; GROUP_COUNT],
            ExistingTable::ReplaceValidated
        ),
        Err(LookupError::Checksum)
    );
    assert_eq!(&atlas[row.clone()], &before);
    atlas[row.clone()].fill(0);
    atlas[ROW_OFFSET] = 1;
    assert_eq!(
        write_table(&mut atlas, &[MISSING; GROUP_COUNT], ExistingTable::Reject),
        Err(LookupError::Header)
    );
    assert_eq!(
        write_new_table(&mut atlas, &[MISSING; GROUP_COUNT]),
        Err(LookupError::ExistingTable)
    );
}

fn same_failure(groups: &[TerrainGroup<'_>; GROUP_COUNT], expected: LookupError) {
    let mut atlas = vec![0xA5; ATLAS_BYTES];
    atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES].fill(0);
    let before = atlas.clone();
    assert_eq!(
        write_table(&mut atlas, groups, ExistingTable::Reject),
        Err(expected)
    );
    assert!(atlas == before);
    assert_eq!(write_new_table(&mut atlas, groups), Err(expected));
    assert!(atlas == before);
}

#[test]
fn new_constructor_retains_exact_input_validation_and_no_partial_copy() {
    let p = native(0, 0);
    for invalid in [
        Placement { width: 0, ..p },
        Placement { height: 0, ..p },
        Placement { page: 2, ..p },
        Placement { x: 1, ..p },
        Placement { y: 1, ..p },
        Placement {
            x: PAGE_SIDE - 2,
            ..p
        },
        Placement {
            y: PAGE_SIDE - 2,
            ..p
        },
        Placement {
            width: PAGE_SIDE,
            ..p
        },
        Placement {
            height: PAGE_SIDE,
            ..p
        },
    ] {
        same_failure(
            &one_group(&[invalid], Some(ACCENTS)),
            LookupError::Rectangle,
        );
    }
    same_failure(&one_group(&[p], None), LookupError::Topology);
    same_failure(&one_group(&[], Some(ACCENTS)), LookupError::Topology);
    same_failure(&one_group(&[p], Some(GRID)), LookupError::Topology);
    same_failure(&one_group(&[p, p], Some(ACCENTS)), LookupError::Overlap);
    let primary = [p];
    let mut cross_group = one_group(&primary, Some(ACCENTS));
    let duplicate = [p];
    cross_group[6] = TerrainGroup {
        placements: &duplicate,
        topology: Some(ACCENTS),
    };
    same_failure(&cross_group, LookupError::Overlap);
    let oversized = vec![p; MAX_ENTRIES + 1];
    same_failure(&one_group(&oversized, Some(ACCENTS)), LookupError::Capacity);
    let mut atlas = vec![0; ATLAS_BYTES + 1];
    for length in [0, ROW_OFFSET, ATLAS_BYTES - 1, ATLAS_BYTES + 1] {
        assert_eq!(
            write_new_table(&mut atlas[..length], &[MISSING; GROUP_COUNT]),
            Err(LookupError::AtlasLength)
        );
        assert_eq!(
            write_table(
                &mut atlas[..length],
                &[MISSING; GROUP_COUNT],
                ExistingTable::Reject
            ),
            Err(LookupError::AtlasLength)
        );
    }
    assert!(atlas.iter().all(|&byte| byte == 0));
}
