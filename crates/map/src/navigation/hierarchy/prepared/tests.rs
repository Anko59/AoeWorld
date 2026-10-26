use super::*;

#[test]
fn prepared_destination_memo_has_a_fixed_entry_ceiling() {
    let atlas = Atlas {
        side: 1,
        overlay: ResourceOverlay::default(),
        destinations: Mutex::default(),
    };
    let component = ((0, 0), 1);
    let parents = BTreeMap::from([(component, None)]);
    for at in 0..MAX_CACHED_CHUNKS + 3 {
        atlas.remember(
            &parents,
            Some(component),
            TileCoord::new(at as i32, 0),
            TileCoord::new(0, 0),
        );
    }
    assert_eq!(atlas.entry_count(), Some(MAX_CACHED_CHUNKS));
}
