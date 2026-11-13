//! Fixed work at launch; ordinary projectile interpolation carries the fan.
use crate::{BuildingKind, GameWorld};
use aoe_core::FIXED_SUBUNITS_PER_TILE;

impl GameWorld {
    pub(super) fn spread_building_arrow(
        &mut self,
        id: u64,
        kind: BuildingKind,
        index: u8,
        count: u8,
    ) {
        if !matches!(kind, BuildingKind::Castle | BuildingKind::TownCenter) || count < 2 {
            return;
        }
        let Some(p) = self.combat.projectiles.get_mut(&id) else {
            return;
        };
        // DE VER 8.9 Castle/TC spawning half-extents: .5 x .5 tiles.
        // Stratify the fan instead of consuming RNG or querying per arrow.
        let span = FIXED_SUBUNITS_PER_TILE;
        let offset = i32::from(index) * span / i32::from(count - 1) - span / 2;
        let dx = p.to.x - p.from.x;
        let dy = p.to.y - p.from.y;
        let norm = dx.abs().max(dy.abs()).max(1);
        let lateral =
            |v: i32, amount: i32| (i64::from(v) * i64::from(amount) / i64::from(norm)) as i32;
        // A diagonal across the DAT's square spawning area reads as a broad
        // row of firing positions in the isometric view, in every direction.
        let launch = [offset, -offset];
        // Spread secondary impacts across the same DAT half-tile area.
        // This is a deterministic visual fan, not accuracy dice (DAT dispersion
        // is zero). Keep the primary on target and alternate the other lanes.
        let lane = i32::from(index.div_ceil(2)) * span / i32::from(count)
            * if index.is_multiple_of(2) { -1 } else { 1 };
        let aim = [lateral(-dy, lane), lateral(dx, lane)];
        let clamp = |value: i32, extent: i32| value.clamp(0, extent * span - 1);
        p.from.x = clamp(p.from.x + launch[0], self.config.width_tiles);
        p.from.y = clamp(p.from.y + launch[1], self.config.height_tiles);
        p.to.x = clamp(p.to.x + aim[0], self.config.width_tiles);
        p.to.y = clamp(p.to.y + aim[1], self.config.height_tiles);
    }
}
