use super::unit_index::ChunkPresence;
use crate::{BuildingKind, DiplomaticStance, EconomyError, GameWorld, Stockpile};
use aoe_core::{ChunkCoord, EntityId, PlayerId, TileCoord};
use aoe_map::ResourceKind;
use std::collections::{BTreeMap, BTreeSet};

mod demo;
mod messages;
mod surrender;
mod transfer;
pub(crate) use messages::History;
pub(crate) use surrender::Conquests;
pub(crate) use transfer::reset_conquered_unit;
#[cfg(test)]
mod tests;

const HOSTILITY_SECONDS: u64 = 10;

pub(crate) type StanceRecord = (PlayerId, PlayerId, DiplomaticStance);

#[derive(Debug, Default)]
pub(crate) struct Diplomacy {
    pub(crate) history: History,
    pub(crate) conquests: Conquests,
    stances: BTreeMap<(PlayerId, PlayerId), DiplomaticStance>,
    pub(crate) last_hostile_ticks: BTreeMap<(PlayerId, PlayerId), u64>,
    // Recipient first, so an AI only reads offers sent to its own seat.
    pub(crate) offers: BTreeSet<(PlayerId, PlayerId)>,
}

impl Diplomacy {
    pub(crate) fn records(&self) -> impl Iterator<Item = StanceRecord> + '_ {
        self.stances
            .iter()
            .map(|(&(from, to), &stance)| (from, to, stance))
    }

    pub(crate) fn restore(&mut self, records: &[StanceRecord]) {
        self.stances = records
            .iter()
            .map(|&(from, to, stance)| ((from, to), stance))
            .collect();
    }

    pub(crate) fn record_hostile_damage(&mut self, from: PlayerId, to: PlayerId, tick: u64) {
        if from != to {
            self.last_hostile_ticks.insert(seat_pair(from, to), tick);
        }
    }

    pub(crate) fn hash(&self, hash: &mut blake3::Hasher) {
        self.history.hash(hash);
        self.conquests.hash(hash);
        if !self.last_hostile_ticks.is_empty() {
            hash.update(b"recent hostile damage\0");
            hash.update(&(self.last_hostile_ticks.len() as u64).to_le_bytes());
            for ((a, b), tick) in &self.last_hostile_ticks {
                hash.update(&a.0.to_le_bytes());
                hash.update(&b.0.to_le_bytes());
                hash.update(&tick.to_le_bytes());
            }
        }
        if !self.offers.is_empty() {
            hash.update(b"alliance offers\0");
            hash.update(&(self.offers.len() as u64).to_le_bytes());
            for (to, from) in &self.offers {
                hash.update(&to.0.to_le_bytes());
                hash.update(&from.0.to_le_bytes());
            }
        }
        if !self.stances.is_empty() {
            hash.update(b"diplomatic stances\0");
            hash.update(&(self.stances.len() as u64).to_le_bytes());
            for (from, to, stance) in self.records() {
                hash.update(&from.0.to_le_bytes());
                hash.update(&to.0.to_le_bytes());
                hash.update(&[stance as u8]);
            }
        }
    }
}

fn seat_pair(a: PlayerId, b: PlayerId) -> (PlayerId, PlayerId) {
    (a.min(b), a.max(b))
}

impl GameWorld {
    pub fn diplomatic_stance(&self, from: PlayerId, to: PlayerId) -> DiplomaticStance {
        if from == to {
            return DiplomaticStance::Ally;
        }
        self.diplomacy
            .stances
            .get(&(from, to))
            .copied()
            .unwrap_or_default()
    }

    #[inline]
    pub(crate) fn is_enemy(&self, from: PlayerId, to: PlayerId) -> bool {
        from != to
            && (self.diplomacy.stances.is_empty()
                || self.diplomatic_stance(from, to) == DiplomaticStance::Enemy)
    }

    /// Direct attacks can target neutrals. Incidental friendly fire against
    /// one's own units is governed by the individual weapon's rules.
    #[inline]
    pub(crate) fn allied_seats(&self, from: PlayerId, to: PlayerId) -> bool {
        from != to
            && !self.diplomacy.stances.is_empty()
            && self.diplomatic_stance(from, to) == DiplomaticStance::Ally
    }

    #[inline]
    pub(crate) fn enemies_present(
        &self,
        presence: &ChunkPresence,
        chunk: ChunkCoord,
        from: PlayerId,
    ) -> bool {
        if self.diplomacy.stances.is_empty() {
            presence.others(chunk, from)
        } else {
            presence.players(chunk).any(|to| self.is_enemy(from, to))
        }
    }

    fn validate_diplomacy_pair(&self, from: PlayerId, to: PlayerId) -> Result<(), EconomyError> {
        if from == to
            || self.surrendered_to(from).is_some()
            || self.surrendered_to(to).is_some()
            || !self.economy.players.contains_key(&from)
            || !self.economy.players.contains_key(&to)
        {
            return Err(EconomyError::InvalidDiplomaticSeat);
        }
        Ok(())
    }

    pub fn set_diplomatic_stance(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        stance: DiplomaticStance,
    ) -> Result<(), EconomyError> {
        self.validate_diplomacy_pair(from, to)?;
        self.diplomacy.offers.remove(&(to, from));
        self.diplomacy.offers.remove(&(from, to));
        let current = self.diplomatic_stance(from, to);
        self.diplomacy.stances.insert((from, to), stance);
        if stance == DiplomaticStance::Ally {
            self.diplomacy.stances.insert((to, from), stance);
        } else if current == DiplomaticStance::Ally {
            // The other seat loses the alliance without declaring hostility.
            self.diplomacy
                .stances
                .insert((to, from), DiplomaticStance::Neutral);
        }
        if stance == DiplomaticStance::Enemy && current != stance {
            self.combat.quiet.rouse_player(from);
        }
        Ok(())
    }

    pub fn alliance_requests(&self, player: PlayerId) -> impl Iterator<Item = PlayerId> + '_ {
        self.diplomacy
            .offers
            .range((player, PlayerId(0))..=(player, PlayerId(u16::MAX)))
            .map(|&(_, from)| from)
    }

    pub fn offered_alliance(&self, from: PlayerId, to: PlayerId) -> bool {
        self.diplomacy.offers.contains(&(to, from))
    }

    pub fn diplomatic_overrides(
        &self,
        player: PlayerId,
    ) -> impl Iterator<Item = (PlayerId, DiplomaticStance)> + '_ {
        self.diplomacy
            .stances
            .range((player, PlayerId(0))..=(player, PlayerId(u16::MAX)))
            .map(|(&(_, to), &stance)| (to, stance))
    }

    /// Default enemy targeting is separate from declaring or actively fighting a war.
    pub fn recently_fought(&self, a: PlayerId, b: PlayerId) -> bool {
        self.diplomacy
            .last_hostile_ticks
            .get(&seat_pair(a, b))
            .is_some_and(|&tick| {
                self.tick.0.saturating_sub(tick)
                    < u64::from(self.config.tick_hz) * HOSTILITY_SECONDS
            })
    }

    pub fn at_war(&self, a: PlayerId, b: PlayerId) -> bool {
        let declared =
            |from, to| self.diplomacy.stances.get(&(from, to)) == Some(&DiplomaticStance::Enemy);
        declared(a, b) || declared(b, a) || self.recently_fought(a, b)
    }

    pub fn offer_alliance(&mut self, from: PlayerId, to: PlayerId) -> Result<(), EconomyError> {
        self.validate_diplomacy_pair(from, to)?;
        if self.allied_seats(from, to) {
            return Ok(());
        }
        if self.at_war(from, to) {
            return Err(EconomyError::DiplomacyAtWar);
        }
        if self.offered_alliance(to, from) {
            self.set_diplomatic_stance(from, to, DiplomaticStance::Ally)
        } else {
            self.diplomacy.offers.insert((to, from));
            Ok(())
        }
    }

    pub fn allied_market_positions(
        &self,
        player: PlayerId,
    ) -> impl Iterator<Item = (PlayerId, TileCoord)> + '_ {
        self.diplomatic_overrides(player)
            .filter(|&(_, stance)| stance == DiplomaticStance::Ally)
            .flat_map(|(owner, _)| self.player_buildings_of(owner, BuildingKind::Market))
            .filter(|b| b.is_complete() && b.hp > 0)
            .map(|b| {
                let rect = b.footprint;
                (
                    b.player,
                    TileCoord::new(
                        rect.min.x + rect.width() / 2,
                        rect.min.y + rect.height() / 2,
                    ),
                )
            })
    }

    pub fn tribute_market(&self, player: PlayerId) -> Option<EntityId> {
        self.player_buildings_of(player, BuildingKind::Market)
            .find(|b| b.is_complete() && b.hp > 0)
            .map(|b| b.id)
    }

    pub fn issue_tribute(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        market: EntityId,
        resource: ResourceKind,
        amount: u32,
    ) -> Result<(), EconomyError> {
        self.validate_diplomacy_pair(from, to)?;
        let building = self
            .economy
            .buildings
            .get(&market)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != from {
            return Err(EconomyError::NotOwner);
        }
        if !building.is_complete() || building.kind != BuildingKind::Market || building.hp == 0 {
            return Err(EconomyError::NoMarket);
        }
        if amount == 0 {
            return Err(EconomyError::InvalidTributeAmount);
        }
        let sender = self.economy.players[&from];
        let fee = u64::from(crate::tribute_fee_permyriad(sender.researched));
        let total = u64::from(amount) + (u64::from(amount) * fee).div_ceil(10_000);
        let mut debit = sender.stockpile;
        let mut credit = self.economy.players[&to].stockpile;
        let balance = resource_amount(&mut debit, resource);
        *balance = u32::try_from(total)
            .ok()
            .and_then(|total| balance.checked_sub(total))
            .ok_or(EconomyError::InsufficientResources)?;
        let balance = resource_amount(&mut credit, resource);
        *balance = balance
            .checked_add(amount)
            .ok_or(EconomyError::InvalidTributeAmount)?;
        self.economy
            .players
            .get_mut(&from)
            .expect("validated sender")
            .stockpile = debit;
        let recipient = self
            .economy
            .players
            .get_mut(&to)
            .expect("validated recipient");
        recipient.stockpile = credit;
        recipient.clear_funded_shortage();
        Ok(())
    }
}

fn resource_amount(stockpile: &mut Stockpile, resource: ResourceKind) -> &mut u32 {
    match resource {
        ResourceKind::Food => &mut stockpile.food,
        ResourceKind::Wood => &mut stockpile.wood,
        ResourceKind::Gold => &mut stockpile.gold,
        ResourceKind::Stone => &mut stockpile.stone,
    }
}
