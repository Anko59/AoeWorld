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

impl super::GameUnit {
    /// All work and strike poses use the authoritative target, including
    /// fractional animal positions, rather than the final path segment.
    pub(crate) fn face_towards(&mut self, target: aoe_core::WorldPosition) {
        self.facing = facing_for(
            i64::from(target.x) - i64::from(self.position.x),
            i64::from(target.y) - i64::from(self.position.y),
            self.facing,
        );
    }

    pub(crate) fn face_tile(&mut self, tile: aoe_core::TileCoord) {
        let scale = aoe_core::FIXED_SUBUNITS_PER_TILE;
        self.face_towards(aoe_core::WorldPosition::new(
            tile.x * scale + scale / 2,
            tile.y * scale + scale / 2,
        ));
    }

    pub(crate) fn face_footprint(&mut self, footprint: aoe_core::TileRect) {
        let from = self.position.tile_floor();
        self.face_tile(aoe_core::TileCoord::new(
            from.x.clamp(footprint.min.x, footprint.max.x - 1),
            from.y.clamp(footprint.min.y, footprint.max.y - 1),
        ));
    }
}
