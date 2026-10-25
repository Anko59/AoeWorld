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
            1536
        } else {
            1024
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

pub(super) fn oriented(
    anchor: WorldPosition,
    offset: WorldPosition,
    heading: WorldPosition,
) -> WorldPosition {
    let x = i64::from(heading.x);
    let y = i64::from(heading.y);
    let length = (x * x + y * y).isqrt().max(1);
    WorldPosition::new(
        anchor
            .x
            .saturating_add(((-y * i64::from(offset.x) + x * i64::from(offset.y)) / length) as i32),
        anchor
            .y
            .saturating_add(((x * i64::from(offset.x) + y * i64::from(offset.y)) / length) as i32),
    )
}
