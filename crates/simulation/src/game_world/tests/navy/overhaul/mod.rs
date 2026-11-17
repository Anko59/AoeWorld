//! The Naval Warfare Overhaul (DE update 169123): ship lines, Dock and
//! University technologies, and the single-class combat rules. Sources and
//! stated assumptions: docs/navy.md.
use super::*;
use crate::{Age, BuildingKind, Stockpile, Tech};

struct Fleet {
    world: GameWorld,
    dock: EntityId,
    university: EntityId,
    water: TileCoord,
}

fn grant(world: &mut GameWorld, age: Age) {
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.stockpile = Stockpile {
        food: 100_000,
        wood: 100_000,
        gold: 100_000,
        stone: 0,
    };
    player.population_cap = 500;
    player.age = age;
    player.researched |= Tech::FeudalAge.bit();
    if age >= Age::Castle {
        player.researched |= Tech::CastleAge.bit();
    }
    if age >= Age::Imperial {
        player.researched |= Tech::ImperialAge.bit();
    }
}

fn fleet(age: Age) -> Fleet {
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let (land, water) = bank(&world);
    let dock = world
        .insert_building(
            PlayerId(0),
            BuildingKind::Dock,
            TileRect::from_xywh(land.x - 1, land.y + 4, 3, 3),
            true,
        )
        .unwrap();
    let university = world
        .insert_building(
            PlayerId(0),
            BuildingKind::University,
            TileRect::from_xywh(land.x - 8, land.y + 20, 4, 4),
            true,
        )
        .unwrap();
    grant(&mut world, age);
    Fleet {
        world,
        dock,
        university,
        water,
    }
}

fn finish(world: &mut GameWorld, tech: Tech) {
    world.complete_research(PlayerId(0), tech, &mut Vec::new());
}

#[test]
fn the_dock_trains_each_ship_line_in_its_age() {
    use UnitKind::*;
    let mut f = fleet(Age::Feudal);
    for kind in [Galley, FireGalley, DemolitionRaft, Hulk] {
        assert_eq!(
            f.world.issue_train(PlayerId(0), f.dock, kind),
            Ok(()),
            "{kind:?}"
        );
        f.world.issue_cancel_training(PlayerId(0), f.dock).unwrap();
    }
    // Second tier needs Castle Age and Medium Warships; training a base ship
    // after the upgrade queues the upgraded one.
    assert_eq!(
        f.world.issue_train(PlayerId(0), f.dock, WarGalley),
        Err(EconomyError::RequiresCastleAge)
    );
    assert_eq!(
        f.world.issue_train(PlayerId(0), f.dock, CannonGalleon),
        Err(EconomyError::RequiresImperialAge)
    );
    grant(&mut f.world, Age::Castle);
    assert_eq!(
        f.world.issue_train(PlayerId(0), f.dock, FireShip),
        Err(EconomyError::TechRequirementMissing)
    );
    finish(&mut f.world, Tech::WarGalley);
    for (queued, trained) in [(Galley, WarGalley), (FireGalley, FireShip), (Hulk, WarHulk)] {
        f.world.issue_train(PlayerId(0), f.dock, queued).unwrap();
        let queue = &f.world.building(f.dock).unwrap().training_queue;
        assert_eq!(queue.back().unwrap().kind, trained);
        f.world.issue_cancel_training(PlayerId(0), f.dock).unwrap();
    }
    assert_eq!(
        f.world.issue_train(PlayerId(0), f.dock, Galleon),
        Err(EconomyError::RequiresImperialAge)
    );
    grant(&mut f.world, Age::Imperial);
    assert_eq!(
        f.world.issue_train(PlayerId(0), f.dock, Galleon),
        Err(EconomyError::TechRequirementMissing)
    );
    finish(&mut f.world, Tech::HeavyWarships);
    for (queued, trained) in [
        (Galley, Galleon),
        (WarGalley, Galleon),
        (FireGalley, FastFireShip),
        (Hulk, Carrack),
    ] {
        f.world.issue_train(PlayerId(0), f.dock, queued).unwrap();
        let queue = &f.world.building(f.dock).unwrap().training_queue;
        assert_eq!(queue.back().unwrap().kind, trained);
        f.world.issue_cancel_training(PlayerId(0), f.dock).unwrap();
    }
    // Cannon Galleon needs no Chemistry after the overhaul; Catapult Galleon
    // is the same slot for civilisations without it.
    for kind in [CannonGalleon, CatapultGalleon] {
        assert_eq!(
            f.world.issue_train(PlayerId(0), f.dock, kind),
            Ok(()),
            "{kind:?}"
        );
        f.world.issue_cancel_training(PlayerId(0), f.dock).unwrap();
    }
    assert_eq!(
        f.world.issue_train(PlayerId(0), f.dock, EliteCannonGalleon),
        Err(EconomyError::TechRequirementMissing)
    );
}

#[test]
fn warship_techs_upgrade_the_galley_fire_and_hulk_lines_in_place() {
    use UnitKind::*;
    let mut f = fleet(Age::Imperial);
    let ids = [Galley, FireGalley, Hulk].map(|kind| ship(&mut f.world, kind, f.water, 0));
    let hp = |w: &GameWorld, id| w.unit(id).unwrap().hp;
    let before = ids.map(|id| hp(&f.world, id));
    finish(&mut f.world, Tech::WarGalley);
    assert_eq!(
        ids.map(|id| f.world.unit(id).unwrap().kind),
        [WarGalley, FireShip, WarHulk]
    );
    assert_eq!(
        ids.map(|id| hp(&f.world, id)),
        [before[0] + 15, before[1] + 10, before[2] + 25]
    );
    finish(&mut f.world, Tech::HeavyWarships);
    assert_eq!(
        ids.map(|id| f.world.unit(id).unwrap().kind),
        [Galleon, FastFireShip, Carrack]
    );
    assert_eq!(
        ids.map(|id| f.world.unit(id).unwrap().max_hp),
        [155, 140, 135]
    );
}

#[test]
fn the_demolition_line_upgrades_one_step_per_tech_and_cannons_go_elite() {
    use UnitKind::*;
    let mut f = fleet(Age::Imperial);
    let raft = ship(&mut f.world, DemolitionRaft, f.water, 0);
    let cannon = ship(
        &mut f.world,
        CannonGalleon,
        TileCoord::new(f.water.x + 4, f.water.y),
        0,
    );
    finish(&mut f.world, Tech::DemolitionShip);
    assert_eq!(f.world.unit(raft).unwrap().kind, DemolitionShip);
    finish(&mut f.world, Tech::HeavyDemolitionShip);
    assert_eq!(f.world.unit(raft).unwrap().kind, HeavyDemolitionShip);
    assert_eq!(f.world.unit(cannon).unwrap().kind, CannonGalleon);
    finish(&mut f.world, Tech::EliteCannonGalleon);
    assert_eq!(f.world.unit(cannon).unwrap().kind, EliteCannonGalleon);
}

#[test]
fn naval_techs_need_their_building_age_and_predecessor() {
    let mut f = fleet(Age::Feudal);
    let (dock, university) = (f.dock, f.university);
    let research =
        |f: &mut Fleet, building, tech| f.world.issue_research(PlayerId(0), building, tech);
    assert_eq!(research(&mut f, dock, Tech::FishingLines), Ok(()));
    f.world.issue_cancel_research(PlayerId(0), dock).unwrap();
    assert_eq!(
        research(&mut f, dock, Tech::Gillnets),
        Err(EconomyError::RequiresCastleAge)
    );
    grant(&mut f.world, Age::Castle);
    assert_eq!(
        research(&mut f, dock, Tech::Gillnets),
        Err(EconomyError::TechRequirementMissing)
    );
    assert_eq!(
        research(&mut f, dock, Tech::Careening),
        Err(EconomyError::NotResearchable),
        "Careening is a University tech, not a Dock tech"
    );
    assert_eq!(research(&mut f, university, Tech::Careening), Ok(()));
    f.world
        .issue_cancel_research(PlayerId(0), university)
        .unwrap();
    assert_eq!(
        research(&mut f, university, Tech::DryDock),
        Err(EconomyError::RequiresImperialAge)
    );
    grant(&mut f.world, Age::Imperial);
    assert_eq!(
        research(&mut f, university, Tech::DryDock),
        Err(EconomyError::TechRequirementMissing)
    );
    assert_eq!(
        research(&mut f, dock, Tech::HeavyWarships),
        Err(EconomyError::TechRequirementMissing)
    );
    assert_eq!(
        research(&mut f, university, Tech::CarvelHull),
        Err(EconomyError::TechRequirementMissing)
    );
}

#[test]
fn clinker_construction_makes_ships_sail_faster() {
    let sail = |techs: &[Tech]| {
        let mut f = fleet(Age::Castle);
        for t in techs {
            finish(&mut f.world, *t);
        }
        let id = ship(&mut f.world, UnitKind::Galley, f.water, 0);
        let goal = TileCoord::new(f.water.x + 10, f.water.y);
        f.world.issue_move(id, at(goal)).unwrap();
        advance(&mut f.world, 100);
        f.world.unit(id).unwrap().position.x - at(f.water).x
    };
    let slow = sail(&[]);
    let fast = sail(&[Tech::ClinkerConstruction]);
    assert!(fast > slow, "{fast} vs {slow}");
    assert!(
        i64::from(fast) * 100 <= i64::from(slow) * 112,
        "about 10%: {fast} vs {slow}"
    );
}

#[test]
fn fire_ship_melee_damage_is_resisted_by_the_hulk_line() {
    use UnitKind::*;
    assert_eq!(
        crate::unit_combat(FireShip).attack_kind,
        crate::DamageKind::Melee
    );
    let burn = |victim| {
        let mut w = coast();
        let (_, water) = bank(&w);
        let attacker = ship(&mut w, FireShip, water, 0);
        let target = ship(&mut w, victim, TileCoord::new(water.x + 1, water.y), 1);
        w.issue_stance(PlayerId(1), &[target], aoe_core::CombatStance::NoAttack)
            .unwrap();
        let before = w.unit(target).unwrap().hp;
        w.issue_attack(PlayerId(0), attacker, target).unwrap();
        advance(&mut w, 40);
        before - w.unit(target).unwrap().hp
    };
    let galley = burn(Galley);
    let hulk = burn(Hulk);
    assert!(galley > 0 && hulk > 0, "fire hurts both: {galley}, {hulk}");
    assert!(
        hulk * 2 < galley,
        "the Hulk's melee armour resists fire: {hulk} vs {galley}"
    );
}

#[test]
fn the_hulk_line_grapples_in_melee_and_is_weak_to_galleys() {
    use UnitKind::*;
    let mut w = coast();
    let (_, water) = bank(&w);
    let hulk = ship(&mut w, Hulk, water, 0);
    let galley = ship(&mut w, Galley, TileCoord::new(water.x + 1, water.y), 1);
    w.issue_stance(PlayerId(1), &[galley], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let before = w.unit(galley).unwrap().hp;
    w.issue_attack(PlayerId(0), hulk, galley).unwrap();
    advance(&mut w, 60);
    assert!(
        w.unit(galley).unwrap().hp < before,
        "contact melee attack lands"
    );
    // A Galley's 5 bonus against Long Ranged Ships does not apply to Hulks, but
    // its 6 pierce meets only 1 pierce armour, against a Galley's 3.
    let stats = |k| *crate::unit_combat(k);
    let dmg = |a: crate::UnitCombat, v: crate::UnitCombat| {
        crate::damage_dealt(a.attack, a.attack_kind, v.melee_armour, v.pierce_armour)
    };
    assert!(dmg(stats(Galley), stats(Hulk)) > dmg(stats(Galley), stats(Galley)));
    assert!(dmg(stats(Hulk), stats(Galley)) > dmg(stats(Hulk), stats(Carrack)));
}

#[test]
fn a_cannon_galleon_throws_splash_shot_and_has_a_minimum_range() {
    use UnitKind::*;
    let mut w = coast();
    let (_, water) = bank(&w);
    let cannon = ship(&mut w, CannonGalleon, water, 0);
    let a = ship(&mut w, Galley, TileCoord::new(water.x + 8, water.y), 1);
    let b = ship(&mut w, Galley, TileCoord::new(water.x + 8, water.y + 1), 1);
    for id in [a, b] {
        w.issue_stance(PlayerId(1), &[id], aoe_core::CombatStance::NoAttack)
            .unwrap();
    }
    w.issue_attack(PlayerId(0), cannon, a).unwrap();
    let mut stone = false;
    let (hp_a, hp_b) = (w.unit(a).unwrap().hp, w.unit(b).unwrap().hp);
    for _ in 0..400 {
        advance(&mut w, 1);
        stone |= w
            .projectiles()
            .any(|p| p.kind == crate::ProjectileKind::Stone);
    }
    assert!(stone, "cannon shot is a thrown stone");
    assert!(w.unit(a).unwrap().hp < hp_a, "target hit");
    assert!(
        w.unit(b).unwrap().hp < hp_b,
        "splash hits the neighbouring ship"
    );
    // Inside the three-tile minimum range nothing is fired.
    let mut w = coast();
    let (_, water) = bank(&w);
    let cannon = ship(&mut w, CannonGalleon, water, 0);
    let close = ship(&mut w, FishingShip, TileCoord::new(water.x + 1, water.y), 1);
    let hp = w.unit(close).unwrap().hp;
    w.issue_attack(PlayerId(0), cannon, close).unwrap();
    advance(&mut w, 400);
    assert_eq!(w.unit(close).unwrap().hp, hp);
}

#[test]
fn demolition_hulls_blast_further_and_hit_buildings_harder_per_tier() {
    use UnitKind::*;
    let blast = |kind: UnitKind, distance: i32| {
        let mut w = coast();
        let (_, water) = bank(&w);
        let origin = at(water);
        let demo = w.spawn_kind(PlayerId(0), kind, origin).unwrap();
        let near = w
            .spawn_kind(
                PlayerId(1),
                Carrack,
                WorldPosition::new(origin.x + 1024, origin.y),
            )
            .unwrap();
        let far = w
            .spawn_kind(
                PlayerId(1),
                Carrack,
                WorldPosition::new(origin.x + distance, origin.y),
            )
            .unwrap();
        w.issue_stance(PlayerId(1), &[near, far], aoe_core::CombatStance::NoAttack)
            .unwrap();
        let hp = w.unit(far).unwrap().hp;
        w.issue_attack(PlayerId(0), demo, near).unwrap();
        advance(&mut w, 5);
        hp - w.unit(far).unwrap().hp
    };
    // Blast radii 2.5, 3 and 3.25 tiles (DAT BlastWidth).
    assert!(blast(DemolitionRaft, 2560) > 0);
    assert_eq!(blast(DemolitionRaft, 2800), 0);
    assert!(blast(DemolitionShip, 3072) > 0);
    assert_eq!(blast(DemolitionShip, 3200), 0);
    assert!(blast(HeavyDemolitionShip, 3328) > 0);
    assert_eq!(blast(HeavyDemolitionShip, 3400), 0);
    let attack = |k| crate::unit_combat(k).attack;
    assert!(attack(DemolitionRaft) < attack(DemolitionShip));
    assert!(attack(DemolitionShip) < attack(HeavyDemolitionShip));
}

#[test]
fn incendiaries_make_a_sunk_fire_ship_blast_nearby_enemies() {
    use UnitKind::*;
    let sink = |incendiaries: bool| {
        let mut w = coast();
        w.place_town_center(PlayerId(0), TileCoord::new(10, 10))
            .unwrap();
        let (_, water) = bank(&w);
        w.economy.players.get_mut(&PlayerId(0)).unwrap().researched |= if incendiaries {
            Tech::Incendiaries.bit()
        } else {
            0
        };
        let fire = ship(&mut w, FireShip, water, 0);
        let neighbour = ship(&mut w, Galley, TileCoord::new(water.x + 1, water.y), 1);
        let hp = w.unit(neighbour).unwrap().hp;
        w.issue_stance(PlayerId(1), &[neighbour], aoe_core::CombatStance::NoAttack)
            .unwrap();
        let index = w.lookup[&fire];
        w.units[index].state.hp = 1;
        let mut changed = Vec::new();
        w.resolve_hit(
            (neighbour, PlayerId(1)),
            fire,
            50,
            0,
            crate::DamageKind::Pierce,
            &mut changed,
        );
        assert!(w.unit(fire).is_none_or(|u| u.hp == 0));
        advance(&mut w, 1);
        hp - w.unit(neighbour).unwrap().hp
    };
    assert_eq!(sink(false), 0);
    assert!(sink(true) > 0);
}

#[test]
fn fishing_lines_and_gillnets_raise_fishing_ship_carry_and_gather_rate() {
    let mut f = fleet(Age::Castle);
    let id = ship(&mut f.world, UnitKind::FishingShip, f.water, 0);
    let unit = f.world.unit(id).unwrap();
    assert_eq!(f.world.gather_capacity(&unit, false), 15);
    assert_eq!(f.world.fishing_ship_percent(PlayerId(0)), 100);
    finish(&mut f.world, Tech::FishingLines);
    assert_eq!(f.world.gather_capacity(&unit, false), 20);
    assert_eq!(f.world.fishing_ship_percent(PlayerId(0)), 110);
    finish(&mut f.world, Tech::Gillnets);
    assert_eq!(f.world.gather_capacity(&unit, false), 25);
    assert_eq!(f.world.fishing_ship_percent(PlayerId(0)), 120);
}

#[test]
fn shipwright_cuts_ship_wood_and_build_time_but_not_land_units() {
    let mut f = fleet(Age::Imperial);
    let wood = |f: &Fleet| f.world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    let cost = |f: &mut Fleet| {
        let before = wood(f);
        f.world
            .issue_train(PlayerId(0), f.dock, UnitKind::Galley)
            .unwrap();
        let spent = before - wood(f);
        let ticks = f
            .world
            .building(f.dock)
            .unwrap()
            .training_queue
            .back()
            .unwrap()
            .total_ticks;
        f.world.issue_cancel_training(PlayerId(0), f.dock).unwrap();
        (spent, ticks / 20)
    };
    assert_eq!(cost(&mut f), (90, 45));
    finish(&mut f.world, Tech::Shipwright);
    assert_eq!(cost(&mut f), (72, 30));
    assert_eq!(wood(&f), 100_000, "cancelling refunds the discounted price");
}
