use super::*;

#[test]
fn immutable_owner_borrows_validated_native_records_without_copy_or_decode() {
    let frames = std::array::from_fn::<_, 6, _>(|i| native(i as u16, (i % 2) as u16));
    let groups = one_group(&frames, Some(GRID));
    let pixels = vec![0; ATLAS_BYTES];
    let original_address = pixels.as_ptr();
    let owner = ConstructedTerrainAtlas::new(pixels, &groups).unwrap();
    assert_eq!(owner.pixels().as_ptr(), original_address);
    let table = owner.table();
    assert_eq!(table.metadata(), decode(owner.pixels()).unwrap().unwrap());
    for (index, expected) in frames.iter().enumerate() {
        assert_eq!(table.descriptor(index as u16).unwrap(), *expected);
    }
    for [x, y] in [[0, 0], [-7, 11], [i32::MIN, i32::MAX]] {
        let index = table.metadata().frame_index(0, x, y).unwrap();
        assert_eq!(table.frame(0, x, y).unwrap(), frames[usize::from(index)]);
    }
    assert_eq!(table.descriptor(6), Err(LookupError::Index));
    assert_eq!(table.frame(6, 0, 0), Err(LookupError::Index));
    assert_eq!(&owner.pixels()[2 * PAGE_BYTES..2 * PAGE_BYTES + 4], &[0; 4]);
}

#[test]
fn consuming_owner_discards_proof_but_preserves_owned_bytes_and_capacity() {
    let owner =
        ConstructedTerrainAtlas::new(vec![0; ATLAS_BYTES], &[MISSING; GROUP_COUNT]).unwrap();
    let address = owner.pixels().as_ptr();
    let metadata = owner.table().metadata();
    let mut raw = owner.into_pixels();
    assert_eq!(raw.as_ptr(), address);
    assert_eq!(decode(&raw), Ok(Some(metadata)));
    raw[ROW_OFFSET + 12] ^= 1;
    assert_eq!(decode(&raw), Err(LookupError::Checksum));
    assert!(matches!(
        ConstructedTerrainAtlas::new(raw, &[MISSING; GROUP_COUNT]),
        Err(LookupError::ExistingTable)
    ));
}

#[test]
fn immutable_owner_cannot_be_created_from_unvalidated_raw_row_or_bad_inputs() {
    let mut raw = vec![0; ATLAS_BYTES];
    raw[ROW_OFFSET + ROW_BYTES - 1] = 1;
    assert!(matches!(
        ConstructedTerrainAtlas::new(raw, &[MISSING; GROUP_COUNT]),
        Err(LookupError::ExistingTable)
    ));
    assert!(matches!(
        ConstructedTerrainAtlas::new(vec![0; ATLAS_BYTES - 1], &[MISSING; GROUP_COUNT]),
        Err(LookupError::AtlasLength)
    ));
    let overlap = [native(0, 0), native(0, 0)];
    assert!(matches!(
        ConstructedTerrainAtlas::new(vec![0; ATLAS_BYTES], &one_group(&overlap, Some(ACCENTS))),
        Err(LookupError::Overlap)
    ));
    let frames = [native(0, 0)];
    assert!(matches!(
        ConstructedTerrainAtlas::new(vec![0; ATLAS_BYTES], &one_group(&frames, Some(GRID))),
        Err(LookupError::Topology)
    ));
}
