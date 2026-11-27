use super::*;
use aoe_core::{PlayerId, Tick};
use serde::{Deserialize, Serialize};

/// The DE 200-year countdown, five game seconds per year.
pub const RELIC_VICTORY_SECONDS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RelicVictory {
    Holding { player: PlayerId, deadline: Tick },
    Won(PlayerId),
}

impl GameWorld {
    pub fn enable_relic_victory(&mut self) {
        self.relics.victory_enabled = true;
        self.update_relic_victory();
    }

    pub fn relic_victory(&self) -> Option<RelicVictory> {
        self.relics.victory
    }

    pub fn relic_winner(&self) -> Option<PlayerId> {
        match self.relics.victory {
            Some(RelicVictory::Won(player)) => Some(player),
            _ => None,
        }
    }

    pub(super) fn update_relic_victory(&mut self) {
        if !self.relics.victory_enabled || self.relic_winner().is_some() {
            return;
        }
        let leader = self.all_relics_owner();
        match (leader, self.relics.victory) {
            (Some(player), Some(RelicVictory::Holding { player: held, .. })) if held == player => {}
            (Some(player), _) => {
                self.relics.victory =
                    Some(RelicVictory::Holding {
                        player,
                        deadline: Tick(self.tick.0.saturating_add(
                            RELIC_VICTORY_SECONDS * u64::from(self.config.tick_hz),
                        )),
                    });
            }
            (None, _) => self.relics.victory = None,
        }
    }

    fn all_relics_owner(&self) -> Option<PlayerId> {
        if !self.relics.ground.is_empty() || !self.relics.carriers.is_empty() {
            return None;
        }
        let mut monasteries = self.relics.stored.keys();
        let owner = self.economy.buildings.get(monasteries.next()?)?.player;
        monasteries
            .all(|id| {
                self.economy
                    .buildings
                    .get(id)
                    .is_some_and(|b| b.player == owner)
            })
            .then_some(owner)
    }

    pub(in crate::game_world) fn advance_relic_victory(&mut self) {
        if let Some(RelicVictory::Holding { player, deadline }) = self.relics.victory
            && self.tick.0 >= deadline.0
        {
            self.relics.victory = Some(RelicVictory::Won(player));
        }
    }
}
