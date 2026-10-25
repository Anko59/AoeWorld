//! Every order the browser can send, as one typed enum.
use crate::model::{BuildingKind, ResourceKind, Tech, TradeResource, UnitKind};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_protocol::GameplayClientMessage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    Garrison {
        unit: EntityId,
        building: EntityId,
    },
    Ungarrison {
        building: EntityId,
        unit: Option<EntityId>,
    },
    Load {
        units: Vec<EntityId>,
        transport: EntityId,
    },
    Unload {
        transport: EntityId,
        to: TileCoord,
    },
    /// Rearrange immediately, retaining the group's route and queued points.
    SetFormation {
        units: Vec<EntityId>,
        formation: crate::Formation,
    },
    /// Share one route and slowest-member speed; queued points retain the group.
    FormationMove {
        units: Vec<EntityId>,
        formation: crate::Formation,
        to: TileCoord,
        queued: bool,
        attack_move: bool,
    },
    AutoScout {
        unit: EntityId,
        enabled: bool,
    },
    /// Prepay (or cancel and refund) one farm reseed at a completed Mill.
    FarmReseed {
        building: EntityId,
        add: bool,
    },
    /// Set a production building's gather point.
    SetRally {
        building: EntityId,
        target: crate::RallyTarget,
    },
    /// Walk to the centre of `tile` (animals the slot owns are herded).
    Move {
        unit: EntityId,
        to: TileCoord,
    },
    /// Walk to an exact fixed-point position.
    MoveTo {
        unit: EntityId,
        to: WorldPosition,
    },
    /// Append a tile-center move to the unit's current movement queue.
    QueueMove {
        unit: EntityId,
        to: TileCoord,
    },
    Stop {
        unit: EntityId,
    },
    /// Work a resource node by id (see `World::resources`).
    Gather {
        unit: EntityId,
        resource: u64,
    },
    /// Place a foundation with its top-left tile at `origin`; every footprint
    /// tile must be explored, or the server returns `RejectedUnexploredPlacement`.
    /// The builders
    /// start work. The cost is paid when the order is accepted.
    Build {
        builders: Vec<EntityId>,
        kind: BuildingKind,
        origin: TileCoord,
    },
    /// Build a cardinal bridge between two land banks (at most forty water tiles).
    BuildBridge {
        builders: Vec<EntityId>,
        from: TileCoord,
        to: TileCoord,
    },
    /// Lay palisade segments along the line from `from` to `to`.
    BuildWall {
        builders: Vec<EntityId>,
        from: TileCoord,
        to: TileCoord,
    },
    /// Send villagers to finish an unfinished foundation of the slot.
    Construct {
        builders: Vec<EntityId>,
        building: EntityId,
    },
    CancelConstruction {
        building: EntityId,
    },
    /// Work a completed farm (one farmer per farm).
    Farm {
        unit: EntityId,
        farm: EntityId,
    },
    /// Queue a unit at a building.
    Train {
        building: EntityId,
        kind: UnitKind,
    },
    CancelTraining {
        building: EntityId,
    },
    Research {
        building: EntityId,
        tech: Tech,
    },
    CancelResearch {
        building: EntityId,
    },
    /// Attack a visible foreign unit or building.
    Attack {
        unit: EntityId,
        target: EntityId,
    },
    /// Hunt a visible animal with villagers.
    Hunt {
        units: Vec<EntityId>,
        animal: EntityId,
    },
    /// Buy 100 of a commodity for gold at an owned, completed Market.
    MarketBuy {
        building: EntityId,
        resource: TradeResource,
    },
    /// Sell 100 of a commodity for gold at an owned, completed Market.
    MarketSell {
        building: EntityId,
        resource: TradeResource,
    },
}

impl Order {
    /// The wire message carrying this order under `sequence`.
    pub fn message(&self, sequence: u64) -> Option<GameplayClientMessage> {
        use GameplayClientMessage as M;
        Some(match self.clone() {
            Order::Garrison { unit, building } => M::GarrisonOrder {
                sequence,
                entity_id: unit,
                building_id: building,
            },
            Order::Ungarrison { building, unit } => M::UngarrisonOrder {
                sequence,
                building_id: building,
                entity_id: unit,
            },
            Order::Load { units, transport } => M::LoadOrder {
                sequence,
                unit_ids: units,
                transport_id: transport,
            },
            Order::Unload { transport, to } => M::UnloadOrder {
                sequence,
                transport_id: transport,
                destination: to,
            },
            Order::SetFormation { units, formation } => M::FormationOrder {
                sequence,
                unit_ids: units,
                formation,
                destination: None,
                queued: false,
                attack_move: false,
            },
            Order::FormationMove {
                units,
                formation,
                to,
                queued,
                attack_move,
            } => M::FormationOrder {
                sequence,
                unit_ids: units,
                formation,
                destination: Some(WorldPosition::from_tile_center(to).ok()?),
                queued,
                attack_move,
            },
            Order::AutoScout { unit, enabled } => M::AutoScoutOrder {
                sequence,
                entity_id: unit,
                enabled,
            },
            Order::FarmReseed { building, add } => M::FarmReseedOrder {
                sequence,
                building_id: building,
                add,
            },
            Order::SetRally { building, target } => M::SetRallyOrder {
                sequence,
                building_id: building,
                target,
            },
            Order::Move { unit, to } => M::MoveOrder {
                sequence,
                entity_id: unit,
                destination: WorldPosition::from_tile_center(to).ok()?,
            },
            Order::MoveTo { unit, to } => M::MoveOrder {
                sequence,
                entity_id: unit,
                destination: to,
            },
            Order::QueueMove { unit, to } => M::QueueMoveOrder {
                sequence,
                entity_id: unit,
                destination: WorldPosition::from_tile_center(to).ok()?,
            },
            Order::Stop { unit } => M::StopOrder {
                sequence,
                entity_id: unit,
            },
            Order::Gather { unit, resource } => M::GatherOrder {
                sequence,
                entity_id: unit,
                resource_id: resource,
            },
            Order::Build {
                builders,
                kind,
                origin,
            } => M::BuildOrder {
                sequence,
                builder_ids: builders,
                kind,
                origin,
                queued: false,
            },
            Order::BuildBridge { builders, from, to } => M::BuildBridgeOrder {
                sequence,
                builder_ids: builders,
                from,
                to,
                queued: false,
            },
            Order::BuildWall { builders, from, to } => M::BuildWallOrder {
                sequence,
                builder_ids: builders,
                from,
                to,
                queued: false,
            },
            Order::Construct { builders, building } => M::ConstructOrder {
                sequence,
                builder_ids: builders,
                building_id: building,
            },
            Order::CancelConstruction { building } => M::CancelConstructionOrder {
                sequence,
                building_id: building,
            },
            Order::Farm { unit, farm } => M::FarmOrder {
                sequence,
                entity_id: unit,
                farm_id: farm,
            },
            Order::Train { building, kind } => M::TrainOrder {
                sequence,
                building_id: building,
                kind,
            },
            Order::CancelTraining { building } => M::CancelTrainingOrder {
                sequence,
                building_id: building,
            },
            Order::Research { building, tech } => M::ResearchOrder {
                sequence,
                building_id: building,
                tech,
            },
            Order::CancelResearch { building } => M::CancelResearch {
                sequence,
                building_id: building,
            },
            Order::Attack { unit, target } => M::AttackOrder {
                sequence,
                entity_id: unit,
                target_id: target,
            },
            Order::Hunt { units, animal } => M::HuntOrder {
                sequence,
                unit_ids: units,
                animal_id: animal,
            },
            Order::MarketBuy { building, resource } => M::MarketBuy {
                sequence,
                building_id: building,
                resource,
            },
            Order::MarketSell { building, resource } => M::MarketSell {
                sequence,
                building_id: building,
                resource,
            },
        })
    }

    /// The resource a `Gather` order of this kind would collect (documentation aid).
    pub fn gathers(kind: ResourceKind) -> &'static str {
        match kind {
            ResourceKind::Food => "food",
            ResourceKind::Wood => "wood",
            ResourceKind::Gold => "gold",
            ResourceKind::Stone => "stone",
        }
    }
}
