//! Technologies: the table, the research queue, Loom, Town Watch, the Feudal
//! Age, refunds and determinism.
use super::buildings::{finish, set_stock, stock, villager_at};
use super::*;
use crate::{
    Age, BuildingKind, DamageKind, EconomyError, FEUDAL_BUILDINGS, TECH_RULES, Tech, UnitKind,
    damage_dealt,
};

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn rich(world: &mut GameWorld) {
    set_stock(world, 1000, 1000, 1000, 1000);
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 50;
}

/// Build and finish a building of `kind` with its top-left tile at `origin`.
fn built(world: &mut GameWorld, kind: BuildingKind, origin: TileCoord) -> EntityId {
    let worker = villager_at(world, origin.x - 1, origin.y);
    let id = world
        .issue_build(PlayerId(0), &[worker], kind, origin)
        .unwrap();
    finish(world, id, 4000);
    id
}

fn run_ticks(world: &mut GameWorld, seconds: u32) {
    advance(world, seconds * world.config().tick_hz);
}

fn research(world: &mut GameWorld, center: EntityId, tech: Tech) -> Result<(), EconomyError> {
    world.issue_research(PlayerId(0), center, tech)
}

#[test]
fn the_tech_table_is_ordered_and_holds_the_dark_age_values() {
    for (index, rules) in TECH_RULES.iter().enumerate() {
        assert_eq!(rules.tech as usize, index);
        assert_eq!(rules.building, BuildingKind::TownCenter);
    }
    let [loom, watch, feudal] = TECH_RULES;
    let paid = |c: crate::Stockpile| (c.food, c.wood, c.gold, c.stone);
    assert_eq!((paid(loom.cost), loom.seconds), ((0, 0, 50, 0), 25));
    assert_eq!((paid(watch.cost), watch.seconds), ((75, 0, 0, 0), 25));
    assert_eq!((paid(feudal.cost), feudal.seconds), ((500, 0, 0, 0), 130));
    assert!(!FEUDAL_BUILDINGS.contains(&BuildingKind::House));
    assert!(!FEUDAL_BUILDINGS.contains(&BuildingKind::PalisadeWall));
    assert!(!FEUDAL_BUILDINGS.contains(&BuildingKind::Outpost));
}

#[test]
fn loom_raises_hit_points_and_armour_of_existing_damaged_and_new_villagers() {
    let (mut world, scout, center) = village();
    let healthy = villager_at(&mut world, 70, 70);
    let hurt = villager_at(&mut world, 71, 70);
    world.units[world.lookup[&hurt]].state.hp = 10;
    assert_eq!(world.unit_armour(healthy), Some((0, 0)));
    set_stock(&mut world, 0, 0, 50, 0);
    research(&mut world, center, Tech::Loom).unwrap();
    assert_eq!(stock(&world), (0, 0, 0, 0), "paid when queued");
    run_ticks(&mut world, 24);
    assert_eq!(world.unit(healthy).unwrap().max_hp, 25, "not yet");
    run_ticks(&mut world, 1);
    assert!(world.has_researched(PlayerId(0), Tech::Loom));
    let (healthy_now, hurt_now) = (world.unit(healthy).unwrap(), world.unit(hurt).unwrap());
    assert_eq!((healthy_now.hp, healthy_now.max_hp), (40, 40));
    // The damage already taken (15) is kept.
    assert_eq!((hurt_now.hp, hurt_now.max_hp), (25, 40));
    assert_eq!(world.unit_armour(healthy), Some((1, 2)));
    let fresh = villager_at(&mut world, 72, 70);
    let fresh = world.unit(fresh).unwrap();
    assert_eq!((fresh.hp, fresh.max_hp), (40, 40));
    // A militia hit (attack 4, melee) now does 3 instead of 4.
    let (melee, pierce) = world.unit_armour(fresh.id).unwrap();
    assert_eq!(damage_dealt(4, DamageKind::Melee, melee, pierce), 3);
    // Only villagers gain from it.
    let scout = world.unit(scout).unwrap();
    assert_eq!((scout.kind, scout.max_hp), (UnitKind::Scout, 45));
    assert_eq!(world.unit_armour(scout.id), Some((0, 2)));
}

#[test]
fn research_without_the_resources_is_refused_and_changes_nothing() {
    let (mut world, _, center) = village();
    set_stock(&mut world, 74, 500, 49, 0);
    let before = world.canonical_hash();
    for tech in [Tech::Loom, Tech::TownWatch] {
        assert_eq!(
            research(&mut world, center, tech),
            Err(EconomyError::InsufficientResources)
        );
    }
    assert_eq!(world.canonical_hash(), before);
    assert!(world.building(center).unwrap().research.is_none());
    set_stock(&mut world, 75, 0, 50, 0);
    research(&mut world, center, Tech::Loom).unwrap();
    research_blocked_by_busy(&mut world, center);
}

fn research_blocked_by_busy(world: &mut GameWorld, center: EntityId) {
    assert_eq!(
        research(world, center, Tech::TownWatch),
        Err(EconomyError::ResearchBusy)
    );
    assert_eq!(
        world.issue_train(PlayerId(0), center, UnitKind::Villager),
        Err(EconomyError::TrainingQueueFull)
    );
}

#[test]
fn town_watch_adds_four_to_the_line_of_sight_of_every_building() {
    let (mut world, _, center) = village();
    rich(&mut world);
    let house = built(&mut world, BuildingKind::House, at(80, 80));
    assert_eq!(world.building(center).unwrap().los, 8);
    set_stock(&mut world, 75, 0, 0, 0);
    research(&mut world, center, Tech::TownWatch).unwrap();
    run_ticks(&mut world, 25);
    assert_eq!(stock(&world).0, 0);
    assert_eq!(world.building(center).unwrap().los, 12);
    assert_eq!(
        world.building(house).unwrap().los,
        BuildingKind::House.rules().los + 4
    );
    set_stock(&mut world, 0, 100, 0, 10);
    let later = built(&mut world, BuildingKind::Outpost, at(90, 90));
    assert_eq!(world.building(later).unwrap().los, 14);
}

#[test]
fn feudal_age_needs_two_building_types_and_sets_the_age() {
    let (mut world, _, center) = village();
    rich(&mut world);
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::TechRequirementMissing)
    );
    // Houses, walls and outposts do not count.
    built(&mut world, BuildingKind::House, at(80, 80));
    built(&mut world, BuildingKind::Outpost, at(90, 90));
    built(&mut world, BuildingKind::House, at(84, 80));
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::TechRequirementMissing)
    );
    // One type, however many of it.
    built(&mut world, BuildingKind::LumberCamp, at(100, 100));
    built(&mut world, BuildingKind::LumberCamp, at(104, 100));
    let refused = world.canonical_hash();
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::TechRequirementMissing)
    );
    assert_eq!(world.canonical_hash(), refused, "a refusal changes nothing");
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().age, Age::Dark);
    built(&mut world, BuildingKind::Mill, at(110, 100));
    let food = stock(&world).0;
    research(&mut world, center, Tech::FeudalAge).unwrap();
    assert_eq!(stock(&world).0, food - 500);
    run_ticks(&mut world, 129);
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().age, Age::Dark);
    run_ticks(&mut world, 1);
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().age, Age::Feudal);
    assert!(world.has_researched(PlayerId(0), Tech::FeudalAge));
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::AlreadyResearched)
    );
}

#[test]
fn cancelling_research_refunds_the_whole_cost_and_frees_the_queue() {
    let (mut world, _, center) = village();
    set_stock(&mut world, 75, 0, 50, 0);
    research(&mut world, center, Tech::Loom).unwrap();
    run_ticks(&mut world, 10);
    world.issue_cancel_research(PlayerId(0), center).unwrap();
    assert_eq!(stock(&world), (75, 0, 50, 0));
    assert!(world.building(center).unwrap().research.is_none());
    // Cancelling an idle building cannot create resources.
    world.issue_cancel_research(PlayerId(0), center).unwrap();
    assert_eq!(stock(&world), (75, 0, 50, 0));
    assert_eq!(
        world.issue_cancel_research(PlayerId(1), center),
        Err(EconomyError::NotOwner)
    );
    run_ticks(&mut world, 30);
    assert!(!world.has_researched(PlayerId(0), Tech::Loom));
    research(&mut world, center, Tech::Loom).unwrap();
}

#[test]
fn a_technology_is_researched_once_and_one_at_a_time_per_player() {
    let (mut world, _, center) = village();
    rich(&mut world);
    let second = world.place_town_center(PlayerId(0), at(120, 60)).unwrap();
    research(&mut world, center, Tech::Loom).unwrap();
    // The other Town Center may not research the same technology meanwhile.
    assert_eq!(
        research(&mut world, second, Tech::Loom),
        Err(EconomyError::ResearchBusy)
    );
    research(&mut world, second, Tech::TownWatch).unwrap();
    run_ticks(&mut world, 25);
    assert_eq!(
        research(&mut world, center, Tech::Loom),
        Err(EconomyError::AlreadyResearched)
    );
    assert_eq!(
        research(&mut world, second, Tech::Loom),
        Err(EconomyError::AlreadyResearched)
    );
    // Training waits out neither: a queue holding villagers refuses research.
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::ResearchBusy)
    );
}

#[test]
fn the_building_must_be_owned_finished_and_able_to_research() {
    let (mut world, _, center) = village();
    rich(&mut world);
    let barracks = built(&mut world, BuildingKind::Barracks, at(100, 100));
    assert_eq!(
        research(&mut world, barracks, Tech::Loom),
        Err(EconomyError::NotResearchable)
    );
    assert_eq!(
        world.issue_research(PlayerId(1), center, Tech::Loom),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        research(&mut world, EntityId(9_999), Tech::Loom),
        Err(EconomyError::UnknownEntity)
    );
    let worker = villager_at(&mut world, 119, 100);
    let mill = world
        .issue_build(PlayerId(0), &[worker], BuildingKind::Mill, at(120, 100))
        .unwrap();
    assert_eq!(
        research(&mut world, mill, Tech::Loom),
        Err(EconomyError::UnderConstruction)
    );
}

#[test]
fn research_replays_deterministically() {
    let run = || {
        let (mut world, _, center) = village();
        rich(&mut world);
        let hurt = villager_at(&mut world, 70, 70);
        world.units[world.lookup[&hurt]].state.hp = 7;
        research(&mut world, center, Tech::Loom).unwrap();
        run_ticks(&mut world, 12);
        world.issue_cancel_research(PlayerId(0), center).unwrap();
        research(&mut world, center, Tech::Loom).unwrap();
        run_ticks(&mut world, 26);
        research(&mut world, center, Tech::TownWatch).unwrap();
        run_ticks(&mut world, 26);
        world.canonical_hash()
    };
    assert_eq!(run(), run());
    let (mut idle, _, _) = village();
    rich(&mut idle);
    run_ticks(&mut idle, 64);
    assert_ne!(run(), idle.canonical_hash());
}

#[test]
fn loom_armour_is_used_by_real_combat_hits() {
    let (mut world, _, center) = village();
    let victim = villager_at(&mut world, 100, 100);
    research(&mut world, center, Tech::Loom).unwrap();
    run_ticks(&mut world, 25);
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(at(101, 100)).unwrap(),
        )
        .unwrap();
    world.issue_attack(PlayerId(1), enemy, victim).unwrap();
    world.advance();
    assert_eq!(world.unit(victim).unwrap().hp, 37);
}

#[test]
fn unfinished_and_foreign_buildings_do_not_qualify_for_feudal() {
    let (mut world, _, center) = village();
    rich(&mut world);
    built(&mut world, BuildingKind::Mill, at(100, 100));
    let worker = villager_at(&mut world, 119, 100);
    let camp = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::LumberCamp,
            at(120, 100),
        )
        .unwrap();
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::TechRequirementMissing)
    );
    finish(&mut world, camp, 1200);
    world.economy.buildings.get_mut(&camp).unwrap().player = PlayerId(1);
    assert_eq!(
        research(&mut world, center, Tech::FeudalAge),
        Err(EconomyError::TechRequirementMissing)
    );
    world.economy.buildings.get_mut(&camp).unwrap().player = PlayerId(0);
    research(&mut world, center, Tech::FeudalAge).unwrap();
}
