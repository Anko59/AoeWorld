use super::raster::raster_changed;
use super::*;
use std::collections::BTreeSet;
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

/// The row sweep before per-player active counts: the
/// output must stay identical.
fn reference_raster(
    sources: &[(PlayerId, TileCoord, i32)],
    width: i32,
    height: i32,
) -> BTreeMap<i32, Vec<HoldingSpan>> {
    let mut events = BTreeMap::<i32, Vec<(i32, usize, bool)>>::new();
    for (id, &(_, c, r)) in sources.iter().enumerate() {
        for y in (c.y - r).max(0)..=(c.y + r).min(height - 1) {
            let dy = y - c.y;
            let mut dx = r;
            while dx * dx + dy * dy > r * r {
                dx -= 1;
            }
            let (x0, x1) = ((c.x - dx).max(0), (c.x + dx + 1).min(width));
            if x0 < x1 {
                events
                    .entry(y)
                    .or_default()
                    .extend([(x0, id, true), (x1, id, false)]);
            }
        }
    }
    events
        .into_iter()
        .map(|(y, mut events)| {
            events.sort_unstable();
            let mut active = BTreeSet::<usize>::new();
            let mut row = Vec::new();
            let mut previous = 0;
            let key = |id: usize, x: i32| {
                let (player, c, _) = sources[id];
                let (dx, dy) = (i64::from(x - c.x), i64::from(y - c.y));
                (dx * dx + dy * dy, player, id)
            };
            for (x, id, add) in events {
                if previous < x
                    && let Some(&first) = active.first()
                {
                    if active.iter().all(|id| sources[*id].0 == sources[first].0) {
                        row.push(HoldingSpan {
                            player: sources[first].0,
                            y,
                            x0: previous,
                            x1: x,
                        });
                    } else {
                        let mut segments = vec![(previous, x, first)];
                        for &candidate in &active {
                            let mut next = Vec::new();
                            for (lo, hi, owner) in segments {
                                let wins = |at| key(candidate, at) < key(owner, at);
                                let left = wins(lo);
                                let right = wins(hi - 1);
                                if left == right {
                                    next.push((lo, hi, if left { candidate } else { owner }));
                                } else {
                                    let (mut a, mut b) = (lo + 1, hi - 1);
                                    while a < b {
                                        let mid = a + (b - a) / 2;
                                        if wins(mid) == left {
                                            a = mid + 1;
                                        } else {
                                            b = mid;
                                        }
                                    }
                                    next.extend([
                                        (lo, a, if left { candidate } else { owner }),
                                        (a, hi, if right { candidate } else { owner }),
                                    ]);
                                }
                            }
                            segments = next;
                        }
                        for (lo, hi, owner) in segments {
                            row.push(HoldingSpan {
                                player: sources[owner].0,
                                y,
                                x0: lo,
                                x1: hi,
                            });
                        }
                    }
                }
                if add {
                    active.insert(id);
                } else {
                    active.remove(&id);
                }
                previous = x;
            }
            coalesce(&mut row);
            (y, row)
        })
        .collect()
}

#[test]
fn the_row_sweep_matches_the_reference_on_dense_mixed_towns() {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for round in 0..40 {
        let count = 1 + next(400) as usize;
        let sources: Vec<_> = (0..count)
            .map(|_| {
                (
                    PlayerId(next(6) as u16),
                    TileCoord::new(next(96) as i32 - 8, next(96) as i32 - 8),
                    next(24) as i32,
                )
            })
            .collect();
        assert_eq!(
            raster(&sources, 80, 80),
            reference_raster(&sources, 80, 80),
            "round {round}"
        );
    }
}

#[test]
fn an_epoch_re_rasters_only_changed_rows_and_matches_a_full_raster() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let source = |next: &mut dyn FnMut(u64) -> u64| {
        (
            PlayerId(next(6) as u16),
            TileCoord::new(next(96) as i32 - 8, next(96) as i32 - 8),
            next(24) as i32,
        )
    };
    let mut sources: Vec<_> = (0..300).map(|_| source(&mut next)).collect();
    let mut rows = raster(&sources, 80, 80);
    for round in 0..60 {
        let old = sources.clone();
        // Buildings finish, fall and change hands between two epochs.
        for _ in 0..1 + next(6) {
            match next(3) {
                0 => sources.push(source(&mut next)),
                1 if !sources.is_empty() => {
                    sources.remove(next(sources.len() as u64) as usize);
                }
                _ => {
                    let at = next(sources.len() as u64) as usize;
                    sources[at].0 = PlayerId(next(6) as u16);
                }
            }
        }
        raster_changed(&mut rows, &old, &sources, 80, 80);
        assert_eq!(rows, raster(&sources, 80, 80), "round {round}");
    }
}
