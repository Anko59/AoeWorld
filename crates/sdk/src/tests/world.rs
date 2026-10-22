use crate::{Age, BuildingKind, CommandResult, Event, Tech, UnitKind, UnitTask, Visibility, World};
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::{
    ConstructionState, FogState, GameplayBuildingState, GameplayPlayerState,
    GameplayServerMessage as M, GameplayUnitState, HitState, MarketPrices, Stockpile,
};

const ME: PlayerId = PlayerId(3);
const FOE: PlayerId = PlayerId(4);

fn welcome() -> M {
    M::Welcome {
        version: 2,
        world_id: 11,
        map_content_hash: None,
        map_metadata: None,
        width_tiles: 200,
        height_tiles: 200,
        coordinate_precision: 1024,
        tick_hz: 20,
        role: aoe_protocol::GameplayRole::Controller,
        primary_unit_id: EntityId(1),
        resume_token: None,
    }
}

fn unit(id: u32, player: PlayerId, kind: UnitKind) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(id),
        player,
        position: WorldPosition::from_tile_center(TileCoord::new(50 + id as i32, 50)).unwrap(),
        moving: false,
        planning: false,
        facing: 0,
        kind,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
    }
}

fn building(id: u32, player: PlayerId, state: ConstructionState) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id),
        player,
        kind: BuildingKind::House,
        footprint: TileRect::from_xywh(60, 60, 2, 2),
        construction: state,
        progress_permille: if state == ConstructionState::Complete {
            1000
        } else {
            100
        },
        hp: 100,
        max_hp: 100,
        los: 4,
        food_remaining: None,
        training_queue: Vec::new(),
        research: None,
        last_hit: None,
        age: aoe_protocol::Age::Dark,
        gate_open: false,
    }
}

fn player(researched: u32, age: aoe_protocol::Age) -> GameplayPlayerState {
    GameplayPlayerState {
        player: ME,
        stockpile: Stockpile {
            food: 200,
            wood: 200,
            gold: 100,
            stone: 200,
        },
        population_used: 4,
        population_cap: 5,
        age,
        researched,
        market_prices: MarketPrices::default(),
    }
}

fn started() -> World {
    let mut world = World::from_welcome(&welcome()).unwrap();
    world.subscription = 1;
    world.apply(M::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![
            unit(1, ME, UnitKind::Scout),
            unit(2, ME, UnitKind::Villager),
        ],
        players: vec![player(0, aoe_protocol::Age::Dark)],
        buildings: vec![building(20, ME, ConstructionState::UnderConstruction)],
    });
    world
}

fn tick(world: &mut World, n: u64, f: impl FnOnce(&mut Delta)) -> Vec<Event> {
    let mut delta = Delta::default();
    f(&mut delta);
    world.apply(M::Tick {
        revision: 1,
        tick: Tick(n),
        changed_units: delta.units,
        removals: delta.removals,
        changed_players: delta.players,
        changed_buildings: delta.buildings,
        building_removals: delta.building_removals,
    })
}

#[derive(Default)]
struct Delta {
    units: Vec<GameplayUnitState>,
    removals: Vec<EntityId>,
    players: Vec<GameplayPlayerState>,
    buildings: Vec<GameplayBuildingState>,
    building_removals: Vec<EntityId>,
}

#[test]
fn the_slot_learns_who_it_is_from_its_snapshot() {
    let world = started();
    assert_eq!(world.me(), Some(ME));
    assert!(world.ready());
    assert_eq!(world.my_units(None).len(), 2);
    assert_eq!(world.my_units(Some(UnitKind::Villager)).len(), 1);
    assert_eq!(world.economy().unwrap().free_population(), 1);
    assert_eq!(world.map_size(), (200, 200));
    assert_eq!(world.world_id(), 11);
}

#[test]
fn deltas_report_spawns_deaths_and_enemies_leaving_view() {
    let mut world = started();
    let events = tick(&mut world, 5, |d| {
        d.units.push(unit(3, ME, UnitKind::Villager));
        d.units.push(unit(40, FOE, UnitKind::Spearman));
    });
    assert_eq!(
        events,
        [Event::UnitSpawned {
            id: EntityId(3),
            kind: UnitKind::Villager
        }]
    );
    assert_eq!(world.visible_enemies().len(), 1);
    assert_eq!(world.visible_enemies()[0].owner(), FOE);

    let mut dying = unit(40, FOE, UnitKind::Spearman);
    dying.task = UnitTask::Dying;
    dying.hp = 0;
    dying.death_tick = Some(Tick(6));
    let events = tick(&mut world, 6, |d| d.units.push(dying));
    assert!(matches!(
        events[..],
        [Event::UnitDied {
            mine: false,
            owner: FOE,
            ..
        }]
    ));
    assert!(
        world.visible_enemies().is_empty(),
        "the dying are not targets"
    );
    // A second delta about the same corpse is not a second death.
    assert!(tick(&mut world, 7, |d| d.units.push(dying)).is_empty());

    tick(&mut world, 8, |d| d.removals.push(EntityId(40)));
    assert!(world.unit(EntityId(40)).is_none());
}

#[test]
fn own_units_hit_and_killed_raise_events() {
    let mut world = started();
    let mut hit = unit(2, ME, UnitKind::Villager);
    hit.last_hit = Some(HitState {
        attacker: EntityId(40),
        tick: Tick(9),
    });
    hit.hp = 10;
    let events = tick(&mut world, 9, |d| d.units.push(hit));
    assert_eq!(
        events,
        [Event::UnderAttack {
            target: EntityId(2),
            attacker: Some(EntityId(40)),
            building: false,
            tile: hit.position.tile_floor(),
        }]
    );
    hit.task = UnitTask::Dying;
    hit.hp = 0;
    hit.death_tick = Some(Tick(10));
    let events = tick(&mut world, 10, |d| d.units.push(hit));
    assert!(matches!(events[..], [Event::UnitDied { mine: true, .. }]));
    assert_eq!(world.my_units(Some(UnitKind::Villager)).len(), 0);
}

#[test]
fn buildings_complete_get_hit_and_are_destroyed() {
    let mut world = started();
    assert!(world.unfinished_buildings().len() == 1);
    let done = building(20, ME, ConstructionState::Complete);
    let events = tick(&mut world, 3, |d| d.buildings.push(done.clone()));
    assert_eq!(
        events,
        [Event::BuildingCompleted {
            id: EntityId(20),
            kind: BuildingKind::House
        }]
    );
    assert!(world.unfinished_buildings().is_empty());
    assert_eq!(
        world.my_complete_buildings(Some(BuildingKind::House)).len(),
        1
    );

    let mut hit = done.clone();
    hit.last_hit = Some(HitState {
        attacker: EntityId(41),
        tick: Tick(4),
    });
    let events = tick(&mut world, 4, |d| d.buildings.push(hit));
    assert!(matches!(
        events[..],
        [Event::UnderAttack { building: true, .. }]
    ));

    let events = tick(&mut world, 5, |d| d.building_removals.push(EntityId(20)));
    assert!(matches!(
        events[..],
        [Event::BuildingDestroyed { mine: true, .. }]
    ));
    assert!(world.my_buildings(None).is_empty());
}

#[test]
fn enemy_buildings_are_fog_memory_until_their_tiles_are_seen() {
    let mut world = started();
    tick(&mut world, 2, |d| {
        d.buildings
            .push(building(30, FOE, ConstructionState::Complete))
    });
    let known = world.known_enemy_buildings();
    assert_eq!(known.len(), 1);
    assert!(!known[0].visible, "no fog message says it is in sight");
    let region = TileRect::from_xywh(50, 50, 20, 20);
    let mut tiles = vec![0_u8; 400];
    tiles[(10 * 20 + 10) as usize] = 2;
    world.apply(M::FogState(FogState {
        revision: 1,
        tick: Tick(2),
        region,
        disabled: false,
        tiles,
        overview: Vec::new(),
    }));
    assert!(world.known_enemy_buildings()[0].visible);
    assert_eq!(
        world.visibility(TileCoord::new(60, 60)),
        Visibility::Visible
    );
}

#[test]
fn explored_ground_is_remembered_after_the_view_moves() {
    let mut world = started();
    let fog = |region: TileRect, lit: usize| {
        let mut tiles = vec![0_u8; (region.width() * region.height()) as usize];
        tiles[lit] = 2;
        M::FogState(FogState {
            revision: 1,
            tick: Tick(1),
            region,
            disabled: false,
            tiles,
            overview: Vec::new(),
        })
    };
    world.apply(fog(TileRect::from_xywh(0, 0, 10, 10), 0));
    assert_eq!(world.visibility(TileCoord::new(0, 0)), Visibility::Visible);
    world.apply(fog(TileRect::from_xywh(100, 100, 10, 10), 5));
    assert_eq!(
        world.visibility(TileCoord::new(0, 0)),
        Visibility::Explored,
        "remembered, no longer in sight"
    );
    assert_eq!(
        world.visibility(TileCoord::new(105, 100)),
        Visibility::Visible
    );
    assert_eq!(
        world.visibility(TileCoord::new(150, 150)),
        Visibility::Unexplored
    );
}

#[test]
fn research_age_and_order_results_surface_as_events() {
    let mut world = started();
    let feudal = Tech::FeudalAge.bit() | Tech::Loom.bit();
    let events = tick(&mut world, 5, |d| {
        d.players.push(player(feudal, aoe_protocol::Age::Feudal))
    });
    assert!(events.contains(&Event::ResearchCompleted { tech: Tech::Loom }));
    assert!(events.contains(&Event::ResearchCompleted {
        tech: Tech::FeudalAge
    }));
    assert!(events.contains(&Event::AgeAdvanced { age: Age::Feudal }));
    let economy = world.economy().unwrap();
    assert!(economy.has_researched(Tech::FeudalAge) && economy.age() == Age::Feudal);

    let events = world.apply(M::CommandAck {
        sequence: 4,
        result: CommandResult::RejectedInsufficientResources,
        applied_tick: Tick(5),
    });
    assert_eq!(
        events,
        [Event::OrderResult {
            sequence: 4,
            result: CommandResult::RejectedInsufficientResources
        }]
    );
    assert_eq!(
        world.apply(M::WorldReset { world_id: 2 }),
        [Event::WorldReset]
    );
}

#[test]
fn messages_for_an_old_subscription_are_ignored() {
    let mut world = started();
    world.subscription = 2;
    let events = tick(&mut world, 30, |d| {
        d.units.push(unit(9, ME, UnitKind::Villager))
    });
    assert!(events.is_empty());
    assert_eq!(world.tick(), 0, "a stale revision does not move the clock");
    assert!(world.unit(EntityId(9)).is_none());
}

#[test]
fn the_strategic_overview_remembers_ground_outside_the_view_window() {
    let mut world = started();
    // A 200-tile map: tile (100, 100) is overview pixel (128, 128).
    let pixel = 128 * 256 + 128;
    world.apply(M::FogState(FogState {
        revision: 1,
        tick: Tick(1),
        region: TileRect::from_xywh(0, 0, 10, 10),
        disabled: false,
        tiles: vec![0; 100],
        overview: vec![(pixel as u16, 1)],
    }));
    assert_eq!(
        world.visibility(TileCoord::new(100, 100)),
        Visibility::Explored
    );
    assert_eq!(
        world.visibility(TileCoord::new(20, 20)),
        Visibility::Unexplored
    );
    // Inside the window the tile-exact state wins over the coarse pixel.
    assert_eq!(
        world.visibility(TileCoord::new(5, 5)),
        Visibility::Unexplored
    );
    assert!(world.fog().known_chunks_for_test().contains(&(3, 3)));
}
