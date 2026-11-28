use super::economy::{MAX_ECONOMY_PLAYERS, MAX_RETAINED_BUILDINGS, bounded};
use aoe_core::{EntityId, PlayerId, TileCoord};
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;

pub use aoe_core::{DiplomacyEntry, DiplomacyMessage, DiplomaticStance, MAX_DIPLOMACY_LOG};

pub const MAX_DIPLOMACY_NAME: usize = 96;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AllianceOffer {
    #[default]
    None,
    Sent,
    Received,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiplomacySeat {
    pub player: PlayerId,
    #[serde(deserialize_with = "bounded_name")]
    pub name: Option<String>,
    pub stance: DiplomaticStance,
    pub offer: AllianceOffer,
}

/// Market location only. No entity ID, building state or fog data.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AlliedMarket {
    pub player: PlayerId,
    pub tile: TileCoord,
}

#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiplomacyView {
    pub revision: u64,
    pub viewer: PlayerId,
    #[serde(deserialize_with = "bounded_seats")]
    pub seats: Vec<DiplomacySeat>,
    #[serde(deserialize_with = "bounded_markets")]
    pub markets: Vec<AlliedMarket>,
    pub tribute_market: Option<EntityId>,
    pub tribute_fee_permyriad: u16,
    #[serde(deserialize_with = "bounded_log")]
    pub log: Vec<DiplomacyEntry>,
    pub surrendered_to: Option<PlayerId>,
}

impl DiplomacyView {
    pub fn stance(&self, player: PlayerId) -> DiplomaticStance {
        if player == self.viewer {
            return DiplomaticStance::Ally;
        }
        self.seat(player)
            .map_or(DiplomaticStance::Enemy, |seat| seat.stance)
    }

    fn seat(&self, player: PlayerId) -> Option<&DiplomacySeat> {
        self.seats
            .binary_search_by_key(&player, |seat| seat.player)
            .ok()
            .map(|at| &self.seats[at])
    }

    pub(super) fn valid(&self) -> bool {
        self.revision > 0
            && self.tribute_fee_permyriad <= 10_000
            && self.log.len() <= MAX_DIPLOMACY_LOG
            && self.log.windows(2).all(|pair| pair[0].id < pair[1].id)
            && self.log.iter().all(|entry| {
                entry.id != 0
                    && entry.from != entry.to
                    && (entry.from == self.viewer || entry.to == self.viewer)
                    && (entry.message == DiplomacyMessage::Surrender
                        || entry.message.valid_request())
            })
            && self.surrendered_to != Some(self.viewer)
            && self.seats.len() <= MAX_ECONOMY_PLAYERS
            && self.markets.len() <= MAX_RETAINED_BUILDINGS
            && self
                .seats
                .windows(2)
                .all(|pair| pair[0].player < pair[1].player)
            && self.seats.iter().all(|seat| {
                seat.name.as_deref().is_none_or(valid_diplomacy_name)
                    && (seat.player != self.viewer || seat.stance == DiplomaticStance::Ally)
                    && (seat.stance != DiplomaticStance::Ally || seat.offer == AllianceOffer::None)
            })
            && self.markets.windows(2).all(|pair| pair[0] < pair[1])
            && self.markets.iter().all(|market| {
                market.player != self.viewer
                    && market.tile.x >= 0
                    && market.tile.y >= 0
                    && self.stance(market.player) == DiplomaticStance::Ally
            })
    }
}

pub fn valid_diplomacy_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= MAX_DIPLOMACY_NAME
        && !name.chars().any(char::is_control)
}

fn bounded_name<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let name = Option::<Cow<'de, str>>::deserialize(d)?;
    if name
        .as_deref()
        .is_some_and(|name| !valid_diplomacy_name(name))
    {
        return Err(serde::de::Error::custom("invalid seat name"));
    }
    Ok(name.map(Cow::into_owned))
}

fn bounded_seats<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DiplomacySeat>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_PLAYERS>(d)
}

fn bounded_markets<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<AlliedMarket>, D::Error> {
    bounded::<D, _, MAX_RETAINED_BUILDINGS>(d)
}

fn bounded_log<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DiplomacyEntry>, D::Error> {
    bounded::<D, _, MAX_DIPLOMACY_LOG>(d)
}
