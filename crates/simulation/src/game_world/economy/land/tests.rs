use super::*;
#[test]
fn discs_union_clip_and_resolve_overlaps_without_double_counting() {
    let sources = [
        (PlayerId(1), TileCoord::new(0, 0), 2),
        (PlayerId(0), TileCoord::new(2, 0), 1),
    ];
    let a = raster(&sources, 4, 4);
    let b = raster(&[sources[1], sources[0], sources[0]], 4, 4);
    assert_eq!(a, b);
    let tiles: i32 = a.values().flatten().map(|s| s.x1 - s.x0).sum();
    assert_eq!(tiles, 8);
    assert_eq!(
        a[&0],
        vec![
            HoldingSpan {
                player: PlayerId(1),
                y: 0,
                x0: 0,
                x1: 1
            },
            HoldingSpan {
                player: PlayerId(0),
                y: 0,
                x0: 1,
                x1: 4
            }
        ]
    );
}
