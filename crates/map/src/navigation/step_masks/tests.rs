use super::*;

#[test]
fn step_indices_follow_the_search_neighbour_order() {
    let offsets = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    for (index, offset) in offsets.into_iter().enumerate() {
        assert_eq!(step_index(offset), Some(index as u32));
    }
    assert_eq!(step_index((0, 0)), None);
    assert_eq!(step_index((2, 0)), None);
}

#[test]
fn stored_masks_are_read_back_until_a_nearby_collision_change() {
    let mut masks = RouteStepMasks::default();
    let tile = TileCoord::new(70, 5);
    assert_eq!(masks.mask(tile), StepMask::Unknown);
    masks.store(200, 100, tile, Some(0b1010_0001));
    masks.store(200, 100, TileCoord::new(3, 3), None);
    masks.store(200, 100, TileCoord::new(500, 3), Some(1));
    assert_eq!(masks.mask(tile), StepMask::Known(0b1010_0001));
    assert_eq!(masks.mask(TileCoord::new(3, 3)), StepMask::Slow);
    assert_eq!(masks.mask(TileCoord::new(500, 3)), StepMask::Unknown);
    // A change two tiles away is not read by the tile's steps.
    masks.invalidate(TileRect::new(TileCoord::new(72, 5), TileCoord::new(73, 6)));
    assert_eq!(masks.mask(tile), StepMask::Known(0b1010_0001));
    // A change on its diagonal neighbour is.
    masks.invalidate(TileRect::new(TileCoord::new(71, 6), TileCoord::new(72, 7)));
    assert_eq!(masks.mask(tile), StepMask::Unknown);
    assert_eq!(masks.mask(TileCoord::new(3, 3)), StepMask::Slow);
    masks.clear();
    assert_eq!(masks.mask(TileCoord::new(3, 3)), StepMask::Unknown);
}
