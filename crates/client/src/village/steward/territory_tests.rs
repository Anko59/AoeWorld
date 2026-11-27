use super::*;
use std::collections::BTreeSet;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn zone_outline_preserves_holes_and_has_no_chunk_seam() {
    let mut left = [0; 32];
    let mut right = [0; 32];
    left[0..3].copy_from_slice(&[3 << 30, 1 << 30, 3 << 30]);
    right[0..3].copy_from_slice(&[1, 1, 1]);
    let edges = outline(&[(TileCoord::new(0, 0), left), (TileCoord::new(1, 0), right)]);
    let units: BTreeSet<_> = edges
        .into_iter()
        .flat_map(|[x, y, x1, y1]| {
            (x..x1)
                .map(move |x| [x, y, x + 1, y])
                .chain((y..y1).map(move |y| [x, y, x, y + 1]))
        })
        .collect();

    for edge in [
        [31, 1, 32, 1],
        [31, 2, 32, 2],
        [31, 1, 31, 2],
        [32, 1, 32, 2],
    ] {
        assert!(
            units.contains(&edge),
            "missing edge around interior hole: {edge:?}"
        );
    }
    assert!(!units.contains(&[32, 0, 32, 1]));
    assert!(!units.contains(&[32, 2, 32, 3]));
    assert_eq!(units.len(), 16);
}

#[wasm_bindgen_test]
fn an_office_shape_is_swapped_atomically_and_can_be_removed() {
    let mut zones = Zones::default();
    let id = EntityId(4);
    let chunk = |x| (TileCoord::new(x, 0), [1; 32]);
    assert!(zones.receive(id, true, true, vec![chunk(1)]));
    let before = zones.edges[&id].clone();
    assert!(!zones.receive(id, true, false, vec![chunk(2)]));
    assert_eq!(zones.edges[&id], before);
    assert!(zones.receive(id, false, true, vec![chunk(3)]));
    assert!(zones.edges[&id].iter().all(|edge| edge[0] >= 64));
    assert!(zones.receive(id, true, true, vec![]));
    assert!(!zones.edges.contains_key(&id));
    assert!(!zones.receive(id, false, true, vec![chunk(4)]));
}
