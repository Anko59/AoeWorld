use super::*;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        version: u16,
        resume_token: Option<ResumeToken>,
    },
    Subscribe {
        revision: u64,
        region: TileRect,
    },
    MoveOrder {
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    },
    Resync {
        revision: u64,
    },
    GatherOrder {
        sequence: u64,
        entity_id: EntityId,
        resource_id: u64,
    },
    TrainOrder {
        sequence: u64,
        building_id: EntityId,
        kind: UnitKind,
    },
    CancelTrainingOrder {
        sequence: u64,
        building_id: EntityId,
    },
    /// Attack another player's unit or building: walk into melee range, then
    /// hit until the target dies. Cancelled by a later move or gather order.
    AttackOrder {
        sequence: u64,
        entity_id: EntityId,
        target_id: EntityId,
    },
    /// Place a foundation of `kind` with its top-left tile at `origin` and send
    /// the named villagers to build it. The cost is paid on acceptance. Use
    /// `BuildWallOrder` for walls.
    BuildOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        kind: BuildingKind,
        origin: TileCoord,
        queued: bool,
    },
    /// Lay wall segments (`kind` is a palisade or stone wall) on the line from
    /// `from` to `to`, both included.
    BuildWallOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        kind: BuildingKind,
        from: TileCoord,
        to: TileCoord,
        queued: bool,
    },
    /// Cancel a foundation; the cost is refunded for the work not yet done.
    CancelConstructionOrder {
        sequence: u64,
        building_id: EntityId,
    },
    /// Send a villager to work a completed farm (one farmer per farm).
    FarmOrder {
        sequence: u64,
        entity_id: EntityId,
        farm_id: EntityId,
    },
    /// Send villagers to finish an unfinished foundation the player owns;
    /// nothing is charged.
    /// Resume construction or repair a damaged own building.
    ConstructOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        building_id: EntityId,
    },
    /// Research `tech` at a completed building; the cost is paid on acceptance.
    ResearchOrder {
        sequence: u64,
        building_id: EntityId,
        tech: Tech,
    },
    /// Cancel the building's research and refund its full cost.
    CancelResearch {
        sequence: u64,
        building_id: EntityId,
    },
    HuntOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        unit_ids: Vec<EntityId>,
        animal_id: EntityId,
    },
    /// Halt an owned unit or sheep at its authoritative position.
    StopOrder {
        sequence: u64,
        entity_id: EntityId,
    },
    /// Buy 100 units of a commodity for gold at a completed, owned Market;
    /// the cost is the current price plus DE's 30% commission.
    MarketBuy {
        sequence: u64,
        building_id: EntityId,
        resource: TradeResource,
    },
    /// Sell 100 units of a commodity for gold at a completed, owned Market;
    /// the payout is the current price minus DE's 30% commission.
    MarketSell {
        sequence: u64,
        building_id: EntityId,
        resource: TradeResource,
    },
    /// Appended variant: authenticate for exactly one match slot with the
    /// token issued at match creation. Unlike `Hello`, an unknown token is an
    /// error, never a lobby join or a spectator seat, so an AI process can
    /// never take a human's slot.
    JoinSlot {
        version: u16,
        token: ResumeToken,
    },
    SetRallyOrder {
        sequence: u64,
        building_id: EntityId,
        target: RallyTarget,
    },
    /// Append a movement destination instead of replacing the current order.
    QueueMoveOrder {
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    },
    /// Prepay or cancel one farm reseed at a Mill.
    FarmReseedOrder {
        sequence: u64,
        building_id: EntityId,
        add: bool,
    },
    AutoScoutOrder {
        sequence: u64,
        entity_id: EntityId,
        enabled: bool,
    },
    BuildBridgeOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        builder_ids: Vec<EntityId>,
        from: TileCoord,
        to: TileCoord,
        queued: bool,
    },
    GarrisonOrder {
        sequence: u64,
        entity_id: EntityId,
        building_id: EntityId,
    },
    UngarrisonOrder {
        sequence: u64,
        building_id: EntityId,
        entity_id: Option<EntityId>,
    },
    LoadOrder {
        sequence: u64,
        #[serde(deserialize_with = "economy::bounded_builders")]
        unit_ids: Vec<EntityId>,
        transport_id: EntityId,
    },
    UnloadOrder {
        sequence: u64,
        transport_id: EntityId,
        destination: TileCoord,
    },
    /// One military group route. No destination changes posture in place.
    FormationOrder {
        sequence: u64,
        #[serde(deserialize_with = "bounded_members")]
        unit_ids: Vec<EntityId>,
        formation: aoe_core::Formation,
        destination: Option<WorldPosition>,
        queued: bool,
        attack_move: bool,
    },
    StanceOrder {
        sequence: u64,
        #[serde(deserialize_with = "bounded_members")]
        unit_ids: Vec<EntityId>,
        stance: CombatStance,
    },
}

fn bounded_members<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<EntityId>, D::Error> {
    economy::bounded::<D, _, { aoe_core::MAX_FORMATION_MEMBERS }>(d)
}

pub(super) fn validate(message: &ClientMessage) -> Result<(), Error> {
    if let ClientMessage::StanceOrder { unit_ids, .. } = message
        && (unit_ids.is_empty()
            || unit_ids.len() > aoe_core::MAX_FORMATION_MEMBERS
            || unit_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != unit_ids.len())
    {
        return Err(Error::InvalidEconomyState);
    }
    if let ClientMessage::FormationOrder {
        unit_ids,
        destination,
        queued,
        attack_move,
        ..
    } = message
    {
        if unit_ids.is_empty()
            || unit_ids.len() > aoe_core::MAX_FORMATION_MEMBERS
            || destination.is_some_and(|p| p.x < 0 || p.y < 0)
            || (destination.is_none() && (*queued || *attack_move))
        {
            return Err(Error::InvalidEconomyState);
        }
        let unique = unit_ids.iter().collect::<std::collections::BTreeSet<_>>();
        if unique.len() != unit_ids.len() {
            return Err(Error::InvalidEconomyState);
        }
    }
    Ok(())
}
