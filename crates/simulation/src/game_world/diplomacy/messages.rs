use super::*;
use aoe_core::{DiplomacyEntry, DiplomacyLog, DiplomacyMessage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct History {
    next_id: u64,
    logs: BTreeMap<PlayerId, DiplomacyLog>,
}

impl History {
    pub(crate) fn valid(&self, players: &BTreeSet<PlayerId>) -> bool {
        self.logs.iter().all(|(&viewer, log)| {
            log.is_bounded()
                && log.entries().windows(2).all(|pair| pair[0].id < pair[1].id)
                && log.entries().iter().all(|entry| {
                    entry.id > 0
                        && entry.id <= self.next_id
                        && entry.from != entry.to
                        && players.contains(&entry.from)
                        && players.contains(&entry.to)
                        && (viewer == entry.from || viewer == entry.to)
                        && (entry.message == DiplomacyMessage::Surrender
                            || entry.message.valid_request())
                })
        })
    }

    pub(super) fn hash(&self, hash: &mut blake3::Hasher) {
        if self.next_id != 0
            && let Ok(bytes) = postcard::to_allocvec(self)
        {
            hash.update(b"diplomacy messages\0");
            hash.update(&bytes);
        }
    }
}

impl GameWorld {
    pub fn diplomacy_log(&self, player: PlayerId) -> &[DiplomacyEntry] {
        self.diplomacy
            .history
            .logs
            .get(&player)
            .map(DiplomacyLog::entries)
            .unwrap_or_default()
    }

    pub fn send_diplomacy(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        message: DiplomacyMessage,
    ) -> Result<(), EconomyError> {
        self.validate_diplomacy_pair(from, to)?;
        if !message.valid_request() || self.diplomacy.history.next_id == u64::MAX {
            return Err(EconomyError::InvalidTarget);
        }
        match message {
            DiplomacyMessage::Reply { request, accepted } => {
                self.answer_diplomacy(from, to, request, accepted)?;
            }
            DiplomacyMessage::RequestStance(_) => {}
            _ if !self.allied_seats(from, to) => return Err(EconomyError::InvalidTarget),
            DiplomacyMessage::AttackPlayer(target) => {
                self.validate_diplomacy_pair(from, target)?;
                self.validate_diplomacy_pair(to, target)?;
                if !self.is_enemy(to, target) {
                    return Err(EconomyError::InvalidTarget);
                }
            }
            DiplomacyMessage::Defend(at)
                if !aoe_core::WorldPosition::from_tile_center(at)
                    .is_ok_and(|pos| self.config.valid_ground_position(pos))
                    || self.visibility(from, at) != crate::TileVisibility::Visible =>
            {
                return Err(EconomyError::InvalidTarget);
            }
            _ => {}
        }
        self.log_diplomacy(from, to, message);
        Ok(())
    }

    fn answer_diplomacy(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        request: u64,
        accepted: bool,
    ) -> Result<(), EconomyError> {
        use DiplomacyMessage::*;
        let log = self.diplomacy_log(from);
        let entry = log
            .iter()
            .find(|e| e.id == request && e.from == to && e.to == from);
        let Some(entry) = entry.copied() else {
            return Err(EconomyError::InvalidTarget);
        };
        if matches!(entry.message, Reply { .. } | Surrender)
            || log
                .iter()
                .any(|e| matches!(e.message, Reply { request: id, .. } if id == request))
        {
            return Err(EconomyError::InvalidTarget);
        }
        if !accepted {
            return Ok(());
        }
        if let RequestStance(stance) = entry.message {
            return match stance {
                DiplomaticStance::Ally if self.at_war(from, to) => {
                    Err(EconomyError::DiplomacyAtWar)
                }
                DiplomaticStance::Ally => self.set_diplomatic_stance(from, to, stance),
                DiplomaticStance::Neutral if self.recently_fought(from, to) => {
                    Err(EconomyError::DiplomacyAtWar)
                }
                DiplomaticStance::Neutral => {
                    self.set_diplomatic_stance(from, to, stance)?;
                    self.set_diplomatic_stance(to, from, stance)
                }
                DiplomaticStance::Enemy => self.set_diplomatic_stance(from, to, stance),
            };
        }
        if !self.allied_seats(from, to) {
            return Err(EconomyError::InvalidTarget);
        }
        if let RequestTribute { resource, amount } = entry.message {
            let market = self.tribute_market(from).ok_or(EconomyError::NoMarket)?;
            self.issue_tribute(from, to, market, resource, amount)?;
        }
        Ok(())
    }

    pub(super) fn log_diplomacy(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        message: DiplomacyMessage,
    ) {
        let history = &mut self.diplomacy.history;
        let Some(id) = history.next_id.checked_add(1) else {
            return;
        };
        history.next_id = id;
        let entry = DiplomacyEntry {
            id,
            from,
            to,
            message,
        };
        for player in [from, to] {
            history.logs.entry(player).or_default().push(entry);
        }
    }
}
