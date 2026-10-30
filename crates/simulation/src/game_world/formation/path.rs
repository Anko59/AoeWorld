//! The shared route polyline, parameterized by integer distance. Members use
//! nearby segments only; no breadcrumbs, terrain rays or member route searches.
use aoe_core::WorldPosition;

#[derive(Debug, Default)]
pub(super) struct Path {
    work: std::sync::Arc<super::super::route_scheduler::GroupWork>,
    pub(super) points: Vec<WorldPosition>,
    pub(super) lengths: Vec<i64>,
}

impl Path {
    pub(super) fn new(
        points: impl IntoIterator<Item = WorldPosition>,
        work: std::sync::Arc<super::super::route_scheduler::GroupWork>,
    ) -> Self {
        let mut path = Self {
            work,
            ..Self::default()
        };
        for point in points {
            path.push(point);
        }
        path
    }

    pub(super) fn push(&mut self, point: WorldPosition) {
        self.work.path(1);
        self.work.path_write();
        let length = self.points.last().map_or(0, |&previous| {
            i64::from(crate::game_path::segment_length(
                i64::from(point.x - previous.x),
                i64::from(point.y - previous.y),
            )) + self.lengths.last().copied().unwrap_or(0)
        });
        if self.points.last() != Some(&point) {
            self.points.push(point);
            self.lengths.push(length);
        }
    }

    pub(super) fn end(&self) -> i64 {
        self.lengths.last().copied().unwrap_or(0)
    }

    pub(super) fn cursor_at(&self, progress: i64) -> usize {
        self.lengths
            .partition_point(|&length| {
                self.work.path(1);
                length < progress
            })
            .saturating_sub(1)
            .min(self.points.len().saturating_sub(2))
    }

    pub(super) fn point(
        &self,
        progress: i64,
        cursor: &mut usize,
    ) -> (WorldPosition, WorldPosition) {
        self.work.path(1);
        if self.points.len() < 2 {
            return (
                self.points
                    .first()
                    .copied()
                    .unwrap_or(WorldPosition::new(0, 0)),
                WorldPosition::new(1024, 0),
            );
        }
        *cursor = (*cursor).min(self.points.len() - 2);
        while *cursor + 2 < self.points.len() && self.lengths[*cursor + 1] < progress {
            self.work.path(1);
            *cursor += 1;
        }
        while *cursor > 0 && self.lengths[*cursor] > progress {
            self.work.path(1);
            *cursor -= 1;
        }
        let from = self.points[*cursor];
        let to = self.points[*cursor + 1];
        let heading = WorldPosition::new(to.x - from.x, to.y - from.y);
        let span = (self.lengths[*cursor + 1] - self.lengths[*cursor]).max(1);
        let along = progress - self.lengths[*cursor];
        let (x, y) = delta(i64::from(heading.x), i64::from(heading.y), along, span);
        (WorldPosition::new(from.x + x, from.y + y), heading)
    }

    /// A stable frame for traffic offsets on a grid staircase. Adjacent
    /// tangents blend continuously at their shared vertex; the route's actual
    /// positions and arc lengths stay unchanged.
    pub(super) fn corridor_point(
        &self,
        progress: i64,
        cursor: &mut usize,
    ) -> (WorldPosition, WorldPosition) {
        let (point, heading) = self.point(progress, cursor);
        if self.points.len() < 2 {
            return (point, heading);
        }
        let at = *cursor;
        let difference =
            |a: WorldPosition, b: WorldPosition| WorldPosition::new(b.x - a.x, b.y - a.y);
        let previous = if at > 0 {
            difference(self.points[at - 1], self.points[at])
        } else {
            heading
        };
        let next = if at + 2 < self.points.len() {
            difference(self.points[at + 1], self.points[at + 2])
        } else {
            heading
        };
        if previous == heading && next == heading {
            return (point, heading);
        }
        let normal = |h: WorldPosition| {
            let (x, y) = (i64::from(h.x), i64::from(h.y));
            let length = (x * x + y * y).isqrt().max(1);
            (x * 1024 / length, y * 1024 / length)
        };
        let (a, b, c) = (normal(previous), normal(heading), normal(next));
        let span = (self.lengths[at + 1] - self.lengths[at]).max(1);
        let t = (progress - self.lengths[at]).clamp(0, span);
        let smooth = WorldPosition::new(
            (((a.0 + b.0) * (span - t) + (b.0 + c.0) * t) / span) as i32,
            (((a.1 + b.1) * (span - t) + (b.1 + c.1) * t) / span) as i32,
        );
        (
            point,
            if smooth == WorldPosition::new(0, 0) {
                heading
            } else {
                smooth
            },
        )
    }

    /// Projection onto a fixed local window. A member cannot jump to a distant
    /// leg at a crossing or cut a bend by selecting a remote route point.
    pub(super) fn project(&self, position: WorldPosition, cursor: &mut usize) -> i64 {
        self.work.path(1);
        if self.points.len() < 2 {
            return 0;
        }
        *cursor = (*cursor).min(self.points.len() - 2);
        let mut best = (i64::MAX, *cursor, 0);
        for at in cursor.saturating_sub(2)..=(*cursor + 2).min(self.points.len() - 2) {
            self.work.path(1);
            let a = self.points[at];
            let b = self.points[at + 1];
            let (dx, dy) = (i64::from(b.x - a.x), i64::from(b.y - a.y));
            let span = (self.lengths[at + 1] - self.lengths[at]).max(1);
            let projected = if dy == 0 && dx.abs() == span {
                i64::from(position.x - a.x) * dx.signum()
            } else if dx == 0 && dy.abs() == span {
                i64::from(position.y - a.y) * dy.signum()
            } else {
                (i64::from(position.x - a.x) * dx + i64::from(position.y - a.y) * dy) / span
            };
            let along = projected.clamp(
                if at == 0 { i64::MIN } else { 0 },
                if at + 2 == self.points.len() {
                    i64::MAX
                } else {
                    span
                },
            );
            let (x, y) = delta(dx, dy, along, span);
            let point = WorldPosition::new(a.x + x, a.y + y);
            let candidate = (squared(position, point), at, self.lengths[at] + along);
            if candidate < best {
                best = candidate;
            }
        }
        *cursor = best.1;
        best.2
    }

    /// A packing transition changes the coordinate frame, so relocate from
    /// the physical body. Walk a bounded local window rather than retaining
    /// virtual progress projected with the old lateral offset.
    pub(super) fn relocate(&self, position: WorldPosition, cursor: &mut usize) -> i64 {
        let mut progress = 0;
        for _ in 0..16 {
            let before = *cursor;
            progress = self.project(position, cursor);
            if *cursor == before {
                break;
            }
        }
        progress
    }

    /// Replace future planning after the anchor's current point, retaining the
    /// common polyline behind it for members still approaching a bend.
    pub(super) fn continue_from(
        &mut self,
        progress: i64,
        anchor: WorldPosition,
        points: impl IntoIterator<Item = WorldPosition>,
    ) {
        let keep = self.lengths.partition_point(|&distance| {
            self.work.path(1);
            distance < progress
        });
        self.points.truncate(keep);
        self.lengths.truncate(keep);
        self.push(anchor);
        if let Some(last) = self.lengths.last_mut() {
            *last = progress;
        }
        for point in points {
            self.push(point);
        }
    }
}

fn delta(dx: i64, dy: i64, along: i64, span: i64) -> (i32, i32) {
    if dy == 0 && dx.abs() == span {
        ((dx.signum() * along) as i32, 0)
    } else if dx == 0 && dy.abs() == span {
        (0, (dy.signum() * along) as i32)
    } else {
        ((dx * along / span) as i32, (dy * along / span) as i32)
    }
}

pub(super) fn squared(a: WorldPosition, b: WorldPosition) -> i64 {
    i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)
}

pub(super) fn distance(a: WorldPosition, b: WorldPosition) -> i64 {
    squared(a, b).isqrt()
}

pub(crate) fn step(from: WorldPosition, to: WorldPosition, budget: i32) -> WorldPosition {
    let length = distance(from, to).max(1);
    if length <= i64::from(budget) {
        return to;
    }
    WorldPosition::new(
        from.x + (i64::from(to.x - from.x) * i64::from(budget) / length) as i32,
        from.y + (i64::from(to.y - from.y) * i64::from(budget) / length) as i32,
    )
}
