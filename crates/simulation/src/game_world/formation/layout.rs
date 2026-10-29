//! Integer local coordinates: x across the front, y along the heading.
use aoe_core::{Formation, WorldPosition};

pub(crate) fn slots(form: Formation, count: usize) -> Vec<WorldPosition> {
    if count == 0 {
        return Vec::new();
    }
    let width = match form {
        Formation::Line | Formation::Staggered => (count * 4).isqrt().max(1),
        Formation::Box => count.isqrt().max(1),
        Formation::Flank => count.div_ceil(2).isqrt().max(1),
    };
    let mut slots = Vec::with_capacity(count);
    for n in 0..count {
        let (column, row) = if form == Formation::Flank {
            let wing = n % 2;
            let p = n / 2;
            let x = (p % width) as i32 + 2;
            (if wing == 0 { -x } else { x }, (p / width) as i32)
        } else {
            ((n % width) as i32, (n / width) as i32)
        };
        let gap = if form == Formation::Staggered {
            1560
        } else {
            // Largest land diameter plus integer rotation clearance.
            1040
        };
        let stagger = if form == Formation::Staggered && row % 2 != 0 {
            gap / 2
        } else {
            0
        };
        slots.push(WorldPosition::new(column * gap + stagger, -row * gap));
    }
    let x = slots.iter().map(|p| i64::from(p.x)).sum::<i64>() / count as i64;
    let y = slots.iter().map(|p| i64::from(p.y)).sum::<i64>() / count as i64;
    for p in &mut slots {
        p.x -= x as i32;
        p.y -= y as i32;
    }
    slots
}

#[derive(Clone, Copy)]
pub(super) struct Rotation {
    x: i64,
    y: i64,
    length: i64,
}
impl Rotation {
    pub(super) fn new(heading: WorldPosition) -> Self {
        let (x, y) = (i64::from(heading.x), i64::from(heading.y));
        let length = if x == 0 {
            y.abs()
        } else if y == 0 {
            x.abs()
        } else {
            (x * x + y * y).isqrt()
        }
        .max(1);
        Self { x, y, length }
    }
    pub(super) fn oriented(self, anchor: WorldPosition, offset: WorldPosition) -> WorldPosition {
        let (across, along) = (i64::from(offset.x), i64::from(offset.y));
        let (dx, dy) = if self.y == 0 && self.x != 0 {
            (self.x.signum() * along, self.x.signum() * across)
        } else if self.x == 0 && self.y != 0 {
            (-self.y.signum() * across, self.y.signum() * along)
        } else {
            (
                (-self.y * across + self.x * along) / self.length,
                (self.x * across + self.y * along) / self.length,
            )
        };
        WorldPosition::new(
            anchor.x.saturating_add(dx as i32),
            anchor.y.saturating_add(dy as i32),
        )
    }
}

pub(super) fn oriented(
    anchor: WorldPosition,
    offset: WorldPosition,
    heading: WorldPosition,
) -> WorldPosition {
    Rotation::new(heading).oriented(anchor, offset)
}
