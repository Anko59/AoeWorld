//! Typed, match-local diplomatic requests and their outcomes.
use crate::{DiplomaticStance, PlayerId, TileCoord};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ResourceKind {
    Food,
    Wood,
    Gold,
    Stone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DiplomacyMessage {
    RequestStance(DiplomaticStance),
    RequestTribute { resource: ResourceKind, amount: u32 },
    AttackPlayer(PlayerId),
    Defend(TileCoord),
    Reply { request: u64, accepted: bool },
    Surrender,
}

impl DiplomacyMessage {
    pub fn valid_request(self) -> bool {
        match self {
            Self::RequestTribute { amount, .. } => amount > 0,
            Self::Defend(at) => at.x >= 0 && at.y >= 0,
            Self::Reply { request, .. } => request > 0,
            Self::Surrender => false,
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiplomacyEntry {
    pub id: u64,
    pub from: PlayerId,
    pub to: PlayerId,
    pub message: DiplomacyMessage,
}

pub const MAX_DIPLOMACY_LOG: usize = 32;

/// The recent messages a seat can read after subscribing or loading a save.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiplomacyLog(Vec<DiplomacyEntry>);

impl DiplomacyLog {
    pub fn entries(&self) -> &[DiplomacyEntry] {
        &self.0
    }

    pub fn is_bounded(&self) -> bool {
        self.0.len() <= MAX_DIPLOMACY_LOG
    }

    pub fn push(&mut self, entry: DiplomacyEntry) {
        if self.0.len() == MAX_DIPLOMACY_LOG {
            self.0.remove(0);
        }
        self.0.push(entry);
    }
}
