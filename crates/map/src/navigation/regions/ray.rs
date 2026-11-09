//! A witnessed path answers cheap connected queries before a global build.
//! Failure says nothing about reachability; the region graph then answers it.
use super::{RegionGround, RegionTile, Regions, label};
use aoe_core::TileCoord;

impl Regions {
    pub(super) fn clear_ray(
        &self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: TileCoord,
    ) -> bool {
        if self.chunk_of(from).is_none() || self.chunk_of(to).is_none() {
            return false;
        }
        let start = ground.tile(from);
        if start == RegionTile::Blocked {
            return false;
        }
        let class = |tile| ground.tile(tile);
        let (dx, dy) = (
            i64::from(to.x) - i64::from(from.x),
            i64::from(to.y) - i64::from(from.y),
        );
        let length = dx.abs().max(dy.abs());
        let mut previous = from;
        for step in 1..=length {
            let next = TileCoord::new(
                (i64::from(from.x) + dx * step / length) as i32,
                (i64::from(from.y) + dy * step / length) as i32,
            );
            let next_class = class(next);
            let admitted = match next_class {
                RegionTile::Blocked => false,
                RegionTile::Open => true,
                RegionTile::Gate(gate) => {
                    player == super::ANY_PLAYER
                        || ground.gate_owner(gate) == player
                        || next_class == start
                }
            };
            if !admitted
                || !label::step_bucket(ground, &class, previous, next)
                    .is_some_and(|limit| self.bucket <= limit)
            {
                return false;
            }
            previous = next;
        }
        true
    }
}
