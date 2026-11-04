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

#[test]
fn nearest_source_keeps_a_high_id_town_center_inside_a_low_id_domain() {
    let rows = raster(
        &[
            (PlayerId(0), TileCoord::new(50, 50), 100),
            (PlayerId(1), TileCoord::new(90, 50), 12),
        ],
        256,
        256,
    );
    assert_eq!(
        rows[&50]
            .iter()
            .find(|s| s.x0 <= 90 && 90 < s.x1)
            .unwrap()
            .player,
        PlayerId(1)
    );
    assert_eq!(
        rows[&50]
            .iter()
            .find(|s| s.x0 <= 50 && 50 < s.x1)
            .unwrap()
            .player,
        PlayerId(0)
    );
}

#[test]
fn clipped_chunk_fragments_coalesce_before_any_wire_limit() {
    let mut spans = (0..7)
        .map(|n| HoldingSpan {
            player: PlayerId(0),
            y: 10,
            x0: n * 32,
            x1: (n + 1) * 32,
        })
        .collect();
    coalesce(&mut spans);
    assert_eq!(
        spans,
        vec![HoldingSpan {
            player: PlayerId(0),
            y: 10,
            x0: 0,
            x1: 224
        }]
    );
}
