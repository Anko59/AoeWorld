#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Facing {
    South = 0,
    SouthEast = 1,
    East = 2,
    NorthEast = 3,
    North = 4,
    NorthWest = 5,
    West = 6,
    SouthWest = 7,
}

pub(crate) fn facing_for(dx: i64, dy: i64, current: Facing) -> Facing {
    let sx = dx - dy;
    let sy = dx + dy;
    if sx == 0 && sy == 0 {
        return current;
    }
    let ax = sx.unsigned_abs();
    let ay = sy.unsigned_abs();
    if ax > ay.saturating_mul(2) {
        if sx > 0 { Facing::East } else { Facing::West }
    } else if ay > ax.saturating_mul(2) {
        if sy > 0 { Facing::South } else { Facing::North }
    } else if sx >= 0 && sy >= 0 {
        Facing::SouthEast
    } else if sx >= 0 {
        Facing::NorthEast
    } else if sy >= 0 {
        Facing::SouthWest
    } else {
        Facing::NorthWest
    }
}
