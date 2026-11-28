use super::*;
use aoe_core::DiplomacyMessage;
use serde::{Deserialize, Serialize};

const SURRENDER_ARMY_THRESHOLD: u32 = 5;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Conquests {
    ai_seats: BTreeSet<PlayerId>,
    // Last seat to destroy a Town Center while the defender has no other one.
    candidates: BTreeMap<PlayerId, PlayerId>,
    surrendered: BTreeMap<PlayerId, PlayerId>,
}

impl Conquests {
    pub(crate) fn valid(&self, players: &BTreeSet<PlayerId>) -> bool {
        self.candidates
            .iter()
            .chain(&self.surrendered)
            .all(|(&loser, &winner)| {
                loser != winner && players.contains(&loser) && players.contains(&winner)
            })
            && self.surrendered.keys().all(|&loser| {
                let mut next = loser;
                for _ in 0..self.surrendered.len() {
                    let Some(&winner) = self.surrendered.get(&next) else {
                        return true;
                    };
                    next = winner;
                }
                !self.surrendered.contains_key(&next)
            })
    }

    pub(super) fn hash(&self, hash: &mut blake3::Hasher) {
        if !self.candidates.is_empty() || !self.surrendered.is_empty() {
            hash.update(b"diplomacy conquests\0");
            hash.update(&(self.candidates.len() as u64).to_le_bytes());
            for (&loser, &winner) in &self.candidates {
                hash.update(&loser.0.to_le_bytes());
                hash.update(&winner.0.to_le_bytes());
            }
            hash.update(&(self.surrendered.len() as u64).to_le_bytes());
            for (&loser, &winner) in &self.surrendered {
                hash.update(&loser.0.to_le_bytes());
                hash.update(&winner.0.to_le_bytes());
            }
        }
    }
}

impl GameWorld {
    pub fn set_diplomacy_ai_seats(&mut self, players: &[PlayerId]) {
        self.diplomacy.conquests.ai_seats = players.iter().copied().collect();
    }

    pub fn surrendered_to(&self, player: PlayerId) -> Option<PlayerId> {
        self.diplomacy.conquests.surrendered.get(&player).copied()
    }

    fn has_town_center(&self, player: PlayerId) -> bool {
        self.player_buildings_of(player, BuildingKind::TownCenter)
            .any(|b| b.is_complete() && b.hp > 0)
    }

    pub(crate) fn town_center_conquered(&mut self, loser: PlayerId, winner: PlayerId) {
        if self.diplomacy.conquests.ai_seats.contains(&loser)
            && self.is_enemy(winner, loser)
            && self.validate_diplomacy_pair(loser, winner).is_ok()
            && !self.has_town_center(loser)
        {
            self.diplomacy.conquests.candidates.insert(loser, winner);
        }
    }

    pub(crate) fn has_pending_conquest(&self) -> bool {
        !self.diplomacy.conquests.candidates.is_empty()
    }

    pub(crate) fn advance_surrender(&mut self, changed: &mut Vec<EntityId>) {
        if !self.has_pending_conquest() {
            return;
        }
        let candidates = std::mem::take(&mut self.diplomacy.conquests.candidates);
        for (loser, winner) in candidates {
            if self.has_town_center(loser) || self.allied_seats(loser, winner) {
                continue;
            }
            let Some(winner) = self.conquest_successor(loser, winner) else {
                continue;
            };
            let army = self.unit_census().get(&loser).map_or(0, |counts| {
                counts
                    .iter()
                    .zip(crate::UNIT_COMBAT)
                    .filter(|(_, stats)| {
                        stats.attack > 0 && stats.kind != crate::UnitKind::Villager
                    })
                    .map(|(&count, _)| count)
                    .sum()
            });
            if army >= SURRENDER_ARMY_THRESHOLD {
                self.diplomacy.conquests.candidates.insert(loser, winner);
            } else {
                self.concede(loser, winner, changed);
            }
        }
    }

    fn conquest_successor(&self, loser: PlayerId, mut winner: PlayerId) -> Option<PlayerId> {
        while let Some(next) = self.surrendered_to(winner) {
            winner = next;
        }
        (winner != loser).then_some(winner)
    }

    fn concede(&mut self, loser: PlayerId, winner: PlayerId, changed: &mut Vec<EntityId>) {
        self.transfer_conquered_assets(loser, winner, changed);
        let spent = self
            .combat
            .projectiles
            .extract_if(.., |_, shot| shot.player == loser);
        for (_, shot) in spent {
            self.combat.impacts.remove(&(shot.impact, shot.id));
        }
        self.diplomacy.conquests.surrendered.insert(loser, winner);
        self.diplomacy
            .offers
            .retain(|&(to, from)| to != loser && from != loser);
        self.diplomacy
            .stances
            .retain(|&(from, to), _| from != loser && to != loser);
        self.log_diplomacy(loser, winner, DiplomacyMessage::Surrender);
    }
}
