//! Things that happened since a bot last looked. The SDK derives them from
//! the deltas the server sends, so they cover exactly what the slot can see.
use crate::model::{Age, BuildingKind, CommandResult, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A unit of the slot came into being (trained, or at the start).
    UnitSpawned { id: EntityId, kind: UnitKind },
    /// A visible unit started dying. `mine` is true for the slot's own units.
    UnitDied {
        id: EntityId,
        kind: UnitKind,
        owner: PlayerId,
        mine: bool,
        tile: TileCoord,
    },
    /// A foundation of the slot became a working building.
    BuildingCompleted { id: EntityId, kind: BuildingKind },
    /// A building left the slot's knowledge because it was destroyed.
    BuildingDestroyed {
        id: EntityId,
        kind: BuildingKind,
        owner: PlayerId,
        mine: bool,
    },
    /// A technology finished researching.
    ResearchCompleted { tech: Tech },
    /// The slot reached a new age.
    AgeAdvanced { age: Age },
    /// One of the slot's units or buildings was hit. `attacker` is the hitter
    /// when the server named one.
    UnderAttack {
        target: EntityId,
        attacker: Option<EntityId>,
        building: bool,
        tile: TileCoord,
    },
    /// The server's verdict on an order, by the sequence `send` returned.
    OrderResult {
        sequence: u64,
        result: CommandResult,
    },
    /// The server reported a problem that did not close the connection.
    ServerError { code: u16, message: String },
    /// The server replaced this world (a new match started): reconnect.
    WorldReset,
}

impl Event {
    /// True for events a bot normally reacts to defensively.
    pub fn is_threat(&self) -> bool {
        matches!(self, Event::UnderAttack { .. })
    }
}
