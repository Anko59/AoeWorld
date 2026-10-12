//! Bounded quadratic corner cuts on connected source segments. Endpoints and
//! stations stay attached to the source reach; page clipping is not a bend.
use crate::RiverSegment;
use std::collections::BTreeMap;

type Endpoint = ([i32; 2], i64, u16);

pub(super) fn shape(segments: &[RiverSegment]) -> Vec<Vec<RiverSegment>> {
    let mut ends = BTreeMap::<Endpoint, Vec<(usize, bool)>>::new();
    for (id, s) in segments.iter().enumerate() {
        ends.entry((s.start, s.station_start, s.width_units))
            .or_default()
            .push((id, false));
        ends.entry((s.end, s.station_end, s.width_units))
            .or_default()
            .push((id, true));
    }
    segments
        .iter()
        .enumerate()
        .map(|(id, s)| {
            let neighbor = |end: bool| {
                let key = if end {
                    (s.end, s.station_end, s.width_units)
                } else {
                    (s.start, s.station_start, s.width_units)
                };
                let list = ends.get(&key)?;
                // Ambiguous branches retain their junction instead of guessing a
                // continuation. Exactly one neighbor is the source polyline.
                if list.len() != 2 {
                    return None;
                }
                list.iter()
                    .find(|(other, _)| *other != id)
                    .map(|(other, end)| (segments[*other], *end))
            };
            let mut middle = *s;
            let mut output = Vec::with_capacity(9);
            if let Some((previous, end)) = neighbor(false) {
                let before = trim(previous, end);
                let after = trim(*s, false);
                arc(
                    &mut output,
                    *s,
                    [before.start, s.start, after.start],
                    s.station_start,
                    after.station_start,
                    4,
                    8,
                );
                middle.start = after.start;
                middle.station_start = after.station_start;
            }
            let mut tail = Vec::new();
            if let Some((next, end)) = neighbor(true) {
                let before = trim(*s, true);
                let after = trim(next, end);
                middle.end = before.start;
                middle.station_end = before.station_start;
                arc(
                    &mut tail,
                    *s,
                    [before.start, s.end, after.start],
                    before.station_start,
                    s.station_end,
                    0,
                    4,
                );
            }
            if middle.start != middle.end {
                output.push(middle);
            }
            output.extend(tail);
            output
        })
        .collect()
}

/// Cut at most six tiles or a quarter of either segment. The quadratic
/// remains in their convex hull, at most 3 tiles from the source polyline.
fn trim(s: RiverSegment, end: bool) -> RiverSegment {
    let (a, b, station_a, station_b) = if end {
        (s.end, s.start, s.station_end, s.station_start)
    } else {
        (s.start, s.end, s.station_start, s.station_end)
    };
    let length = (i64::from(b[0]) - i64::from(a[0]))
        .abs()
        .max((i64::from(b[1]) - i64::from(a[1])).abs())
        .max(1);
    let cut = (length / 4).min(6 * 256);
    RiverSegment {
        start: [0, 1]
            .map(|i| (i64::from(a[i]) + (i64::from(b[i]) - i64::from(a[i])) * cut / length) as i32),
        station_start: station_a + (station_b - station_a) * cut / length,
        ..s
    }
}

fn arc(
    out: &mut Vec<RiverSegment>,
    source: RiverSegment,
    points: [[i32; 2]; 3],
    station_a: i64,
    station_b: i64,
    low: i64,
    high: i64,
) {
    let point = |t: i64| {
        [0, 1].map(|i| {
            (((8 - t) * (8 - t) * i64::from(points[0][i])
                + 2 * t * (8 - t) * i64::from(points[1][i])
                + t * t * i64::from(points[2][i]))
                / 64) as i32
        })
    };
    let station = |t: i64| station_a + (station_b - station_a) * (t - low) / (high - low);
    for t in low..high {
        let (start, end) = (point(t), point(t + 1));
        if start != end {
            out.push(RiverSegment {
                start,
                end,
                station_start: station(t),
                station_end: station(t + 1),
                ..source
            });
        }
    }
}
