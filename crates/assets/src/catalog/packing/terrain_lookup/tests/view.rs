use super::*;

#[test]
fn immutable_validated_view_reads_native_records_and_signed_phase() {
    let placements: [Placement; 6] = std::array::from_fn(|i| native(i as u16, 1));
    let mut atlas = vec![0; ATLAS_BYTES];
    let groups = one_group(&placements, Some(GRID));
    let metadata = write_table(&mut atlas, &groups, ExistingTable::Reject).unwrap();
    {
        let view = validated_table(&atlas).unwrap().unwrap();
        assert_eq!(view.metadata(), metadata);
        for index in 0..6 {
            assert_eq!(view.descriptor(index), Ok(placements[usize::from(index)]));
        }
        for x in -1024..1024 {
            for y in [-1024, -1, 0, 1, 1024, i32::MIN, i32::MAX] {
                let index = GRID.periodic_frame(i64::from(x), i64::from(y), 6).unwrap();
                assert_eq!(view.frame(0, x, y), Ok(placements[index as usize]));
            }
        }
        assert_eq!(view.descriptor(6), Err(LookupError::Index));
        assert_eq!(view.frame(1, 0, 0), Err(LookupError::Index));
        assert_eq!(view.frame(7, 0, 0), Err(LookupError::Index));
    }
    // Replacement requires the immutable borrow to end first. The new view
    // never retains old cached groups or allows a corrupt replacement through.
    let replacement = one_group(&placements[..1], Some(ACCENTS));
    let metadata = write_table(&mut atlas, &replacement, ExistingTable::ReplaceValidated).unwrap();
    let view = validated_table(&atlas).unwrap().unwrap();
    assert_eq!(view.metadata(), metadata);
    assert_eq!(view.frame(0, i32::MIN, i32::MAX), Ok(placements[0]));
    assert_eq!(view.descriptor(1), Err(LookupError::Index));
    atlas[ROW_OFFSET + HEADER_BYTES] ^= 1;
    assert!(validated_table(&atlas).is_err());
}

#[test]
fn validated_view_legacy_zero_row_and_exact_atlas_length() {
    let atlas = vec![0; ATLAS_BYTES];
    assert!(validated_table(&atlas).unwrap().is_none());
    assert!(matches!(
        validated_table(&atlas[..ATLAS_BYTES - 1]),
        Err(LookupError::AtlasLength)
    ));
}
