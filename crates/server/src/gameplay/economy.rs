#[path = "economy/holdings.rs"]
pub(super) mod holdings;
use super::GameplayService;
#[path = "economy/kinds.rs"]
mod kinds;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{
    Age, BuildingKind, CarriedResource, ConstructionState, GameplayBuildingState,
    GameplayPlayerState, GameplayUnitState, HitState, MAX_SUBSCRIBED_UNITS, ResearchState,
    ResourceKind, Stockpile, Tech, TradeResource, TrainingState, UnitKind, UnitTask,
};
use aoe_simulation::{GameUnit, GameWorld};
pub(super) use kinds::{native_building, native_kind, native_tech};
use kinds::{wire_building, wire_kind, wire_tech};
use std::collections::BTreeMap;

/// Economy units stay authoritative while offscreen, for selection and worker
/// counts. Their population is bounded by the village rules; population-only
/// workloads continue using the spatial query and the primary scout.
pub(super) fn query_units(
    world: &GameWorld,
    region: TileRect,
    primary_id: EntityId,
    player: PlayerId,
) -> Vec<GameUnit> {
    let mut units = world.query(region).0;
    if world.player_economy(player).is_some() {
        units.extend(
            world
                .units()
                .filter(|unit| unit.player == player)
                .take(MAX_SUBSCRIBED_UNITS + 1),
        );
    } else if let Some(primary) = world.unit(primary_id) {
        units.push(primary);
    }
    units.sort_unstable_by_key(|unit| unit.id);
    units.dedup_by_key(|unit| unit.id);
    units.retain(|unit| world.unit_visible_to(player, unit.id));
    units
}

fn wire_hit(hit: aoe_simulation::HitRecord) -> HitState {
    HitState {
        attacker: hit.attacker,
        tick: hit.tick,
    }
}
fn wire_resource(kind: aoe_map::ResourceKind) -> ResourceKind {
    match kind {
        aoe_map::ResourceKind::Food => ResourceKind::Food,
        aoe_map::ResourceKind::Wood => ResourceKind::Wood,
        aoe_map::ResourceKind::Gold => ResourceKind::Gold,
        aoe_map::ResourceKind::Stone => ResourceKind::Stone,
    }
}
/// Home Castle, managers and General status. The wire carries them for living
/// units only: a Dying body keeps its simulation records until it is removed,
/// and one such unit used to make every Snapshot and Tick fail validation.
pub(super) fn command_state(world: &GameWorld, unit: &GameUnit, state: &mut GameplayUnitState) {
    if unit.hp == 0 {
        return;
    }
    state.home_castle = world.general_castle(unit.id);
    state.managed_by = world.steward_controller(unit.id);
    state.commanded_by = world.general_controller(unit.id);
    state.general = world.general_status(unit.id).map(wire_general);
}
pub(super) fn wire_general(g: aoe_simulation::GeneralStatus) -> aoe_protocol::GeneralStatus {
    aoe_protocol::GeneralStatus {
        army_size: g.army_size,
        state: match g.state {
            aoe_simulation::GeneralState::Idle => aoe_protocol::GeneralState::Idle,
            aoe_simulation::GeneralState::Marching => aoe_protocol::GeneralState::Marching,
            aoe_simulation::GeneralState::Fighting => aoe_protocol::GeneralState::Fighting,
            aoe_simulation::GeneralState::Retreating => aoe_protocol::GeneralState::Retreating,
        },
        training: g.training,
        queued: g.queued,
        mix: g.mix,
        counts: g.counts,
        squads: g.squads.map(|s| {
            s.map(|s| aoe_protocol::SquadStatus {
                leader: s.leader,
                kind: match s.kind {
                    aoe_simulation::SquadKind::Infantry => aoe_protocol::SquadKind::Infantry,
                    aoe_simulation::SquadKind::Archers => aoe_protocol::SquadKind::Archers,
                    aoe_simulation::SquadKind::Cavalry => aoe_protocol::SquadKind::Cavalry,
                    aoe_simulation::SquadKind::Siege => aoe_protocol::SquadKind::Siege,
                },
                size: s.size,
                state: match s.state {
                    aoe_simulation::GeneralState::Idle => aoe_protocol::GeneralState::Idle,
                    aoe_simulation::GeneralState::Marching => aoe_protocol::GeneralState::Marching,
                    aoe_simulation::GeneralState::Fighting => aoe_protocol::GeneralState::Fighting,
                    aoe_simulation::GeneralState::Retreating => {
                        aoe_protocol::GeneralState::Retreating
                    }
                },
            })
        }),
    }
}
pub(super) fn unit_state(world: &GameWorld, unit: &GameUnit) -> GameplayUnitState {
    let mut state = wire_unit(
        unit,
        world.gathering_kind(unit.id),
        world.hunting_target(unit.id),
    );
    state.cargo_count = world.transport_cargo(unit.id).len() as u8;
    command_state(world, unit, &mut state);
    state
}
/// A unit as the wire sees it, given the two facts only the economy knows.
pub(super) fn wire_unit(
    unit: &GameUnit,
    gathering: Option<aoe_map::ResourceKind>,
    hunt_target: Option<EntityId>,
) -> GameplayUnitState {
    GameplayUnitState {
        id: unit.id,
        player: unit.player,
        position: unit.position,
        moving: unit.moving && unit.hp > 0,
        planning: unit.planning && unit.hp > 0,
        facing: unit.facing as u8,
        kind: wire_kind(unit.kind),
        // A body at 0 HP is Dying whatever a late movement or job pass wrote to
        // its task: the wire requires it, and a mismatch used to fail every
        // Snapshot and Tick that carried the corpse.
        task: match if unit.hp == 0 {
            aoe_simulation::UnitTask::Dying
        } else {
            unit.task
        } {
            aoe_simulation::UnitTask::Idle => UnitTask::Idle,
            aoe_simulation::UnitTask::Moving => UnitTask::Moving,
            aoe_simulation::UnitTask::Gathering => UnitTask::Gathering,
            aoe_simulation::UnitTask::Carrying => UnitTask::Carrying,
            aoe_simulation::UnitTask::Returning => UnitTask::Returning,
            aoe_simulation::UnitTask::Building => UnitTask::Building,
            aoe_simulation::UnitTask::Seeding => UnitTask::Seeding,
            aoe_simulation::UnitTask::Farming => UnitTask::Farming,
            aoe_simulation::UnitTask::Attacking => UnitTask::Attacking,
            aoe_simulation::UnitTask::Dying => UnitTask::Dying,
            aoe_simulation::UnitTask::Hunting => UnitTask::Hunting,
            aoe_simulation::UnitTask::Shepherding => UnitTask::Shepherding,
            aoe_simulation::UnitTask::Fishing => UnitTask::Fishing,
        },
        gathering: gathering.map(wire_resource),
        hp: unit.hp,
        max_hp: unit.max_hp,
        attack_target: unit.attack_target.filter(|_| unit.hp > 0),
        last_hit: unit.last_hit.map(wire_hit),
        death_tick: unit.death_tick,
        hunt_target,
        auto_scout: unit.auto_scout,
        sergeant: unit.sergeant,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: unit.stance,
        death_hunt: unit.death_hunt.map(|kind| match kind {
            aoe_simulation::AnimalKind::Sheep => aoe_protocol::AnimalKind::Sheep,
            aoe_simulation::AnimalKind::Deer => aoe_protocol::AnimalKind::Deer,
            aoe_simulation::AnimalKind::Boar => aoe_protocol::AnimalKind::Boar,
            aoe_simulation::AnimalKind::Wolf => aoe_protocol::AnimalKind::Wolf,
        }),
        carried: unit.carried.map(|load| CarriedResource {
            kind: wire_resource(load.kind),
            amount: load.amount,
        }),
    }
}
pub(super) fn native_trade(resource: TradeResource) -> aoe_map::ResourceKind {
    match resource {
        TradeResource::Wood => aoe_map::ResourceKind::Wood,
        TradeResource::Food => aoe_map::ResourceKind::Food,
        TradeResource::Stone => aoe_map::ResourceKind::Stone,
    }
}
fn wire_market_prices(prices: aoe_simulation::MarketPrices) -> aoe_protocol::MarketPrices {
    aoe_protocol::MarketPrices {
        wood: prices.wood,
        food: prices.food,
        stone: prices.stone,
    }
}
pub(super) fn players(world: &GameWorld) -> BTreeMap<PlayerId, GameplayPlayerState> {
    if world.players().next().is_none() {
        return BTreeMap::new();
    }
    let census = world.unit_census();
    world
        .players()
        .map(|state| {
            let mut unit_counts = [0; UnitKind::ALL.len()];
            if let Some(counts) = census.get(&state.player) {
                for (out, kind) in unit_counts.iter_mut().zip(UnitKind::ALL) {
                    *out = counts[native_kind(kind) as usize];
                }
            }
            (state.player, {
                let mut p =
                    player_state(&state, world.recent_deliveries(state.player), unit_counts);
                (p.held_tiles, p.held_share_ppm) = world.held_land(state.player);
                p
            })
        })
        .collect()
}
pub(super) fn player(world: &GameWorld, viewer: PlayerId) -> Option<GameplayPlayerState> {
    let state = world.player_economy(viewer)?;
    let census = world.unit_census();
    let row = census.get(&viewer);
    let counts = std::array::from_fn(|index| {
        row.map_or(0, |row| row[native_kind(UnitKind::ALL[index]) as usize])
    });
    let mut report = player_state(&state, world.recent_deliveries(viewer), counts);
    (report.held_tiles, report.held_share_ppm) = world.held_land(viewer);
    Some(report)
}

fn player_state(
    state: &aoe_simulation::PlayerEconomy,
    recent_deliveries: [u64; 4],
    unit_counts: [u32; UnitKind::ALL.len()],
) -> GameplayPlayerState {
    GameplayPlayerState {
        player: state.player,
        stockpile: Stockpile {
            food: state.stockpile.food,
            wood: state.stockpile.wood,
            gold: state.stockpile.gold,
            stone: state.stockpile.stone,
        },
        population_used: state.population_used,
        population_cap: state.population_cap,
        age: match state.age {
            aoe_simulation::Age::Dark => Age::Dark,
            aoe_simulation::Age::Feudal => Age::Feudal,
            aoe_simulation::Age::Castle => Age::Castle,
            aoe_simulation::Age::Imperial => Age::Imperial,
        },
        researched: state.researched,
        market_prices: wire_market_prices(state.market_prices),
        recent_deliveries,
        unit_counts,
        held_tiles: 0,
        held_share_ppm: 0,
    }
}
pub(super) fn building_in_camera(
    state: &GameplayBuildingState,
    player: PlayerId,
    region: TileRect,
) -> bool {
    state.player == player
        || (state.footprint.min.x < region.max.x
            && state.footprint.max.x > region.min.x
            && state.footprint.min.y < region.max.y
            && state.footprint.max.y > region.min.y)
}

pub(super) fn buildings(
    world: &GameWorld,
    player: PlayerId,
) -> BTreeMap<EntityId, GameplayBuildingState> {
    world
        .known_buildings(player)
        .iter()
        .map(|state| {
            (
                state.id,
                wire_building_state(state, (!world.fog_disabled()).then_some(player)),
            )
        })
        .collect()
}
pub(super) fn subscription_buildings(
    world: &GameWorld,
    player: PlayerId,
    region: TileRect,
) -> BTreeMap<EntityId, GameplayBuildingState> {
    if !world.fog_disabled() {
        return buildings(world, player);
    }
    world
        .buildings()
        .filter(|b| {
            b.player == player
                || (b.footprint.min.x < region.max.x
                    && b.footprint.max.x > region.min.x
                    && b.footprint.min.y < region.max.y
                    && b.footprint.max.y > region.min.y)
        })
        .take(aoe_protocol::MAX_ECONOMY_BUILDINGS + 1)
        .map(|b| (b.id, wire_building_state(b, None)))
        .collect()
}

pub(super) fn wire_building_state(
    state: &aoe_simulation::GameBuilding,
    viewer: Option<PlayerId>,
) -> GameplayBuildingState {
    GameplayBuildingState {
        age: match state.age {
            aoe_simulation::Age::Dark => aoe_protocol::Age::Dark,
            aoe_simulation::Age::Feudal => aoe_protocol::Age::Feudal,
            aoe_simulation::Age::Castle => aoe_protocol::Age::Castle,
            aoe_simulation::Age::Imperial => aoe_protocol::Age::Imperial,
        },
        id: state.id,
        gate_open: state.gate_open,
        rally_point: state
            .rally_point
            .filter(|_| viewer.is_none_or(|player| state.player == player))
            .map(|r| aoe_protocol::RallyPoint {
                position: r.position,
                target: wire_rally(r.target),
            }),
        player: state.player,
        kind: wire_building(state.kind),
        footprint: state.footprint,
        bridge_span: state.bridge_span,
        construction: match state.state {
            aoe_simulation::ConstructionState::UnderConstruction => {
                ConstructionState::UnderConstruction
            }
            aoe_simulation::ConstructionState::Complete => ConstructionState::Complete,
        },
        progress_permille: state.progress_permille(),
        hp: state.hp,
        max_hp: state.max_hp,
        los: state.los,
        food_remaining: state.food_remaining,
        farm_reseeds: state.farm_reseeds,
        garrison: state
            .garrison
            .iter()
            .filter(|_| viewer.is_none_or(|player| state.player == player))
            .map(|g| aoe_protocol::GarrisonUnitState {
                vassal: g
                    .vassal
                    .filter(|_| g.unit.hp > 0)
                    .map(|v| aoe_protocol::VassalReport {
                        villagers: v.villagers,
                        military: v.military,
                        development: v.development,
                        developed: v.developed,
                        alert: v.alert,
                        next: v.next.map(wire_building),
                        wait: v.wait as u8,
                        completed: v.completed,
                        total: v.total,
                        responding: v.responding,
                    }),
                id: g.unit.id,
                kind: wire_kind(g.unit.kind),
                hp: g.unit.hp,
                max_hp: g.unit.max_hp,
                // A Dying occupant carries no report, as no Dying unit carries
                // command fields: the wire requires a living Steward.
                steward: g
                    .steward
                    .filter(|_| g.unit.hp > 0)
                    .map(|v| aoe_protocol::StewardReport {
                        weights: v.settings.weights,
                        cap: v.settings.cap,
                        training: v.settings.training,
                        managed: v.managed,
                        residents: v.residents,
                        wait: match v.wait {
                            aoe_simulation::StewardWait::Growing => {
                                aoe_protocol::StewardWait::Growing
                            }
                            aoe_simulation::StewardWait::Census => {
                                aoe_protocol::StewardWait::Census
                            }
                            aoe_simulation::StewardWait::Full => aoe_protocol::StewardWait::Full,
                            aoe_simulation::StewardWait::NoTownCenter => {
                                aoe_protocol::StewardWait::NoTownCenter
                            }
                            aoe_simulation::StewardWait::Population => {
                                aoe_protocol::StewardWait::Population
                            }
                            aoe_simulation::StewardWait::Paused => {
                                aoe_protocol::StewardWait::Paused
                            }
                            aoe_simulation::StewardWait::Food => aoe_protocol::StewardWait::Food,
                        },
                    }),
            })
            .collect(),
        last_hit: state.last_hit.map(wire_hit),
        research: state.research.map(|entry| ResearchState {
            tech: wire_tech(entry.tech),
            progress_ticks: entry.progress_ticks,
            total_ticks: entry.total_ticks,
        }),
        training_queue: state
            .training_queue
            .iter()
            .map(|entry| TrainingState {
                kind: wire_kind(entry.kind),
                progress_ticks: entry.progress_ticks,
                total_ticks: entry.total_ticks,
            })
            .collect(),
    }
}

impl GameplayService {
    pub(super) async fn advance_world(
        &self,
        mut world: tokio::sync::OwnedRwLockWriteGuard<GameWorld>,
    ) -> Option<tokio::sync::OwnedRwLockWriteGuard<GameWorld>> {
        let hash = self.map_content_hash;
        let directory = self.resource_directory.clone();
        let journal = self.resource_journal.clone();
        let probe = self.probe.clone();
        let advance = move || {
            let mut phase_started = std::time::Instant::now();
            world.advance_with_probe(
                &mut |transaction| {
                    if let (Some(hash), Some(directory)) = (hash, &directory) {
                        match super::resources::save_gathering(
                            directory,
                            &transaction.snapshot(hash),
                            transaction.previous_revision(),
                        ) {
                            Ok(_) => true,
                            Err(error) => {
                                tracing::warn!("gathering persistence refused: {error}");
                                false
                            }
                        }
                    } else {
                        true
                    }
                },
                &mut |phase| {
                    let now = std::time::Instant::now();
                    let name = match phase {
                        aoe_simulation::TickPhase::Movement => "movement",
                        aoe_simulation::TickPhase::Spacing => "spacing",
                        aoe_simulation::TickPhase::Wildlife => "wildlife",
                        aoe_simulation::TickPhase::Combat => "combat",
                        aoe_simulation::TickPhase::Construction => "construction",
                        aoe_simulation::TickPhase::Gathering => "gathering",
                        aoe_simulation::TickPhase::ResearchTraining => "research_training",
                        aoe_simulation::TickPhase::StewardEconomy => "steward_economy",
                        aoe_simulation::TickPhase::BuildingMemory => "building_memory",
                        aoe_simulation::TickPhase::Changed => "changed",
                    };
                    probe.phase(name, now.duration_since(phase_started));
                    phase_started = now;
                },
            );
            let journal_started = std::time::Instant::now();
            let changes = world.take_resource_changes();
            let mut journal = journal.lock().unwrap_or_else(|error| error.into_inner());
            for change in changes {
                journal.record(change);
            }
            drop(journal);
            probe.phase("resource_journal", journal_started.elapsed());
            world
        };
        if self.resource_directory.is_some() || self.map_content_hash.is_some() {
            tokio::task::spawn_blocking(advance).await.ok()
        } else {
            Some(advance())
        }
    }
}

/// Transport conversion stays at the adapter boundary.
pub(super) fn native_rally(target: aoe_protocol::RallyTarget) -> aoe_simulation::RallyTarget {
    use aoe_protocol::RallyTarget as W;
    use aoe_simulation::RallyTarget as N;
    match target {
        W::Ground(p) => N::Ground(p),
        W::Resource(id) => N::Resource(id),
        W::Building(id) => N::Building(id),
        W::Animal(id) => N::Animal(id),
    }
}
fn wire_rally(target: aoe_simulation::RallyTarget) -> aoe_protocol::RallyTarget {
    use aoe_protocol::RallyTarget as W;
    use aoe_simulation::RallyTarget as N;
    match target {
        N::Ground(p) => W::Ground(p),
        N::Resource(id) => W::Resource(id),
        N::Building(id) => W::Building(id),
        N::Animal(id) => W::Animal(id),
    }
}

#[path = "economy/tests.rs"]
#[cfg(test)]
mod tests;
