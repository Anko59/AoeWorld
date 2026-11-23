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
            // The widest dx with dx² + dy² ≤ r².
            let dx = (r * r - dy * dy).max(0).isqrt();
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
        .map(|(y, events)| (y, sweep_row(sources, y, events)))
        .collect()
}

/// Brings `rows`, the raster of `old`, to the raster of `new` by sweeping only
/// the stretch of each row a changed source covers. A tile's owner depends on
/// the sources covering it, by distance and player alone (a source's index only
/// orders sources of one player), so the result equals `raster(new)`. On the
/// owner's Anatolia 4 (83,554 sources) whole-row sweeps of about 164 dirty
/// rows cost 41 ms a second, and copying and sorting both lists 16 ms (#978).
pub(super) fn raster_changed(
    rows: &mut BTreeMap<i32, Vec<HoldingSpan>>,
    old: &[(PlayerId, TileCoord, i32)],
    new: &[(PlayerId, TileCoord, i32)],
    width: i32,
    height: i32,
) {
    let (before, after) = (sorted(old), sorted(new));
    let mut dirty = BTreeMap::<i32, Vec<(i32, i32)>>::new();
    let mut mark = |&(_, c, r): &(PlayerId, TileCoord, i32)| {
        for y in (c.y - r).max(0)..=(c.y + r).min(height - 1) {
            if let Some(span) = span_of(c, r, y, width) {
                dirty.entry(y).or_default().push(span);
            }
        }
    };
    let (mut a, mut b) = (0, 0);
    while a < before.len() || b < after.len() {
        match (before.get(a), after.get(b)) {
            (Some(x), Some(y)) if x == y => (a, b) = (a + 1, b + 1),
            (Some(x), Some(y)) if x < y => {
                mark(x);
                a += 1;
            }
            (Some(x), None) => {
                mark(x);
                a += 1;
            }
            (_, Some(y)) => {
                mark(y);
                b += 1;
            }
            (None, None) => break,
        }
    }
    for windows in dirty.values_mut() {
        windows.sort_unstable();
        let mut merged: Vec<(i32, i32)> = Vec::with_capacity(windows.len());
        for &(x0, x1) in windows.iter() {
            match merged.last_mut() {
                Some(last) if x0 <= last.1 => last.1 = last.1.max(x1),
                _ => merged.push((x0, x1)),
            }
        }
        *windows = merged;
    }
    // Events of each window, clipped to it, from the sources that reach it.
    let mut events = BTreeMap::<(i32, usize), Vec<(i32, usize, bool)>>::new();
    for (id, &(_, c, r)) in new.iter().enumerate() {
        let (top, bottom) = ((c.y - r).max(0), (c.y + r).min(height - 1));
        if top > bottom {
            continue;
        }
        for (&y, windows) in dirty.range(top..=bottom) {
            let Some((x0, x1)) = span_of(c, r, y, width) else {
                continue;
            };
            for (at, &(w0, w1)) in windows.iter().enumerate() {
                let (lo, hi) = (x0.max(w0), x1.min(w1));
                if lo < hi {
                    events
                        .entry((y, at))
                        .or_default()
                        .extend([(lo, id, true), (hi, id, false)]);
                }
            }
        }
    }
    for (y, windows) in dirty {
        let mut row = Vec::new();
        // What lies outside every window is unchanged.
        for span in rows.remove(&y).into_iter().flatten() {
            let mut from = span.x0;
            for &(w0, w1) in &windows {
                if w1 <= from || w0 >= span.x1 {
                    continue;
                }
                if from < w0 {
                    row.push(HoldingSpan {
                        x0: from,
                        x1: w0,
                        ..span
                    });
                }
                from = from.max(w1);
            }
            if from < span.x1 {
                row.push(HoldingSpan { x0: from, ..span });
            }
        }
        for at in 0..windows.len() {
            if let Some(events) = events.remove(&(y, at)) {
                row.extend(sweep_row(new, y, events));
            }
        }
        coalesce(&mut row);
        if !row.is_empty() {
            rows.insert(y, row);
        }
    }
}

/// `list` in order, copied only when it is not already.
fn sorted(
    list: &[(PlayerId, TileCoord, i32)],
) -> std::borrow::Cow<'_, [(PlayerId, TileCoord, i32)]> {
    if list.is_sorted() {
        std::borrow::Cow::Borrowed(list)
    } else {
        let mut list = list.to_vec();
        list.sort_unstable();
        std::borrow::Cow::Owned(list)
    }
}

/// The tiles a disc at `c` of radius `r` covers on row `y`, if any.
fn span_of(c: TileCoord, r: i32, y: i32, width: i32) -> Option<(i32, i32)> {
    let dy = y - c.y;
    // The widest dx with dx² + dy² ≤ r².
    let dx = (r * r - dy * dy).max(0).isqrt();
    let (x0, x1) = ((c.x - dx).max(0), (c.x + dx + 1).min(width));
    (x0 < x1).then_some((x0, x1))
}

/// One row: spans of a single owner directly, nearest-source splits where
/// players' discs overlap. Active sources are counted per player, so a
/// one-owner span costs no scan of everything active.
fn sweep_row(
    sources: &[(PlayerId, TileCoord, i32)],
    y: i32,
    mut events: Vec<(i32, usize, bool)>,
) -> Vec<HoldingSpan> {
    events.sort_unstable();
    let mut active = BTreeSet::<usize>::new();
    let mut owners = BTreeMap::<PlayerId, usize>::new();
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
            if owners.len() == 1 {
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
        let player = sources[id].0;
        if add {
            active.insert(id);
            *owners.entry(player).or_default() += 1;
        } else {
            active.remove(&id);
            if let Some(count) = owners.get_mut(&player) {
                *count -= 1;
                if *count == 0 {
                    owners.remove(&player);
                }
            }
        }
        previous = x;
    }
    coalesce(&mut row);
    row
}
