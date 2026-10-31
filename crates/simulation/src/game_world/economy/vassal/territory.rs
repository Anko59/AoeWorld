//! Territory split between one player's Vassal Castles: a point belongs to the
//! nearest active castle within the radius (integer squared distance), ties to
//! the lower Vassal ID. No point ever belongs to two Vassals.
use super::{Home, VASSAL_RULES};
use crate::GameWorld;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, WorldPosition};

fn d2(a: WorldPosition, b: WorldPosition) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}
impl Home {
    /// Inside the circle and not closer to (or tied but lower-ID than) another
    /// active castle of the same player.
    pub(crate) fn owns(&self, at: WorldPosition) -> bool {
        self.contains(at) && {
            let mine = d2(self.center, at);
            !self.rivals.iter().any(|&(id, c)| {
                let theirs = d2(c, at);
                theirs < mine || (theirs == mine && id < self.id)
            })
        }
    }
}
impl GameWorld {
    /// Recompute each active domain's nearby rivals for one player. Runs only
    /// when a domain activates or deactivates, never per tick.
    pub(super) fn refresh_vassal_territory(&mut self, player: PlayerId) {
        let members: Vec<(EntityId, WorldPosition)> = self
            .economy
            .vassals
            .by_player
            .get(&player)
            .map(|m| m.iter().map(|(id, at)| (*id, *at)).collect())
            .unwrap_or_default();
        let reach = i64::from(2 * VASSAL_RULES.radius_tiles * FIXED_SUBUNITS_PER_TILE);
        for &(id, center) in &members {
            let rivals = members
                .iter()
                .filter(|&&(other, at)| other != id && d2(center, at) <= reach * reach)
                .copied()
                .collect();
            if let Some(home) = self.economy.vassal_homes.get_mut(&id) {
                home.rivals = rivals;
            }
        }
    }
}
