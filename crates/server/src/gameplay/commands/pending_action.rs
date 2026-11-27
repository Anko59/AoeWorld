//! Orders queued for the next tick, as the session received them.
use super::*;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) enum PendingAction {
    VassalDevelopment {
        vassal_id: EntityId,
        enabled: bool,
    },
    VassalFocus {
        vassal_id: EntityId,
        focus: aoe_protocol::VassalFocus,
    },
    Stance {
        unit_ids: Vec<EntityId>,
        stance: aoe_core::CombatStance,
    },
    Garrison {
        entity_id: EntityId,
        building_id: EntityId,
    },
    BackToWork(EntityId),
    TownBell(EntityId),
    Ungarrison {
        building_id: EntityId,
        entity_id: Option<EntityId>,
    },
    Trade(EntityId, EntityId),
    Load {
        unit_ids: Vec<EntityId>,
        transport_id: EntityId,
    },
    Unload {
        transport_id: EntityId,
        destination: aoe_core::TileCoord,
    },
    Formation {
        unit_ids: Vec<EntityId>,
        formation: aoe_core::Formation,
        destination: Option<WorldPosition>,
        queued: bool,
        attack_move: bool,
    },
    /// One selection order: the same order for each unit, acknowledged once.
    Each(Vec<PendingAction>),
    AutoScout {
        entity_id: EntityId,
        enabled: bool,
    },
    FarmReseed {
        building_id: EntityId,
        add: bool,
    },
    StewardDismiss {
        steward_id: EntityId,
    },
    StewardSettings {
        steward_id: EntityId,
        settings: aoe_simulation::StewardSettings,
    },
    SetRally {
        building_id: EntityId,
        target: aoe_protocol::RallyTarget,
    },
    Stop {
        entity_id: EntityId,
    },
    Delete {
        entity_ids: Vec<EntityId>,
    },
    GeneralMoveAlone {
        entity_id: EntityId,
        destination: WorldPosition,
    },
    GeneralTraining {
        entity_id: EntityId,
        enabled: bool,
    },
    GeneralMix {
        entity_id: EntityId,
        mix: [u8; 5],
    },
    Hunt {
        unit_ids: Vec<EntityId>,
        animal_id: EntityId,
    },
    Move {
        entity_id: EntityId,
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
    },
    Gather {
        entity_id: EntityId,
        resource_id: u64,
    },
    CancelTraining {
        building_id: EntityId,
    },
    Attack {
        entity_id: EntityId,
        target_id: EntityId,
    },
    Train {
        building_id: EntityId,
        kind: aoe_protocol::UnitKind,
    },
    Build {
        builder_ids: Vec<EntityId>,
        kind: aoe_protocol::BuildingKind,
        origin: aoe_core::TileCoord,
        queued: bool,
    },
    BuildBridge {
        builder_ids: Vec<EntityId>,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
        queued: bool,
    },
    BuildWall {
        builder_ids: Vec<EntityId>,
        kind: aoe_protocol::BuildingKind,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
        queued: bool,
    },
    Construct {
        builder_ids: Vec<EntityId>,
        building_id: EntityId,
    },
    CancelConstruction {
        building_id: EntityId,
    },
    Farm {
        entity_id: EntityId,
        farm_id: EntityId,
    },
    Research {
        building_id: EntityId,
        tech: aoe_protocol::Tech,
    },
    CancelResearch {
        building_id: EntityId,
    },
    /// Single trades are one lot; bulk orders carry up to `MARKET_MAX_LOTS`.
    MarketTrade {
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
        buy: bool,
        lots: u32,
    },
    Patrol(EntityId, WorldPosition),
    MoveSelection(SelectionMove),
    AttackSelection {
        entity_ids: Vec<EntityId>,
        target_id: EntityId,
    },
    TrainBatch {
        building_ids: Vec<EntityId>,
        kind: aoe_protocol::UnitKind,
    },
    StopSelection(Vec<EntityId>),
    Relic(aoe_protocol::RelicAction),
}
