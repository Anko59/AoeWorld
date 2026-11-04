//! Sparse row sweep with nearest-source Voronoi splits inside influence discs.
use super::*;
use std::collections::BTreeSet;

pub(super) fn coalesce(spans: &mut Vec<HoldingSpan>) {
    spans.sort_unstable_by_key(|s| (s.y, s.x0, s.player));
    let mut out: Vec<HoldingSpan> = Vec::new();
    for s in spans.drain(..) {
        if let Some(last) = out
            .last_mut()
            .filter(|p| p.y == s.y && p.player == s.player && p.x1 == s.x0)
        {
            last.x1 = s.x1;
        } else {
            out.push(s);
        }
    }
    *spans = out;
}

pub(super) fn raster(
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
