use super::*;
use aoe_core::Seed;
use std::collections::BTreeSet;

#[test]
fn coastal_roster_tiers_cover_every_defined_production_kind() {
    let mut offered = BTreeSet::new();
    let seed = Seed(1);
    let (package, elevation, water) = GameWorld::naval_battle_environment(seed).unwrap();
    for stage in 0..TIERS.len() {
        let mut world = GameWorld::from_prepared_map(
            package.clone(),
            elevation.clone(),
            water.clone(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        world.prepare_content_roster(stage, false).unwrap();
        let sites: Vec<_> = world
            .player_buildings(SEATS[0])
            .map(|building| (building.id, building.kind))
            .collect();
        let player = world.player_economy(SEATS[0]).unwrap();
        for (id, site) in sites {
            for &unit in crate::trainable_units(site) {
                if unit.upgraded(player.researched) != unit
                    || unit.required_age() > player.age
                    || unit
                        .required_tech()
                        .is_some_and(|tech| !world.has_researched(SEATS[0], tech))
                {
                    continue;
                }
                world.issue_train(SEATS[0], id, unit).unwrap();
                offered.insert(unit as u8);
            }
        }
    }
    let expected = crate::BUILDING_RULES
        .iter()
        .flat_map(|rule| rule.trainable)
        .map(|&kind| kind as u8)
        .collect();
    assert_eq!(offered, expected);
}

#[test]
fn coastal_castle_trains_a_trebuchet_that_stays_packed_until_ordered() {
    let (package, elevation, water) =
        GameWorld::naval_battle_environment(aoe_scenario::SMOKE.seed).unwrap();
    let mut world =
        GameWorld::from_prepared_map(package, elevation, water, Vec::new(), Vec::new()).unwrap();
    world.prepare_content_roster(0, false).unwrap();
    let castle = world
        .player_buildings(SEATS[0])
        .find(|building| building.kind == BuildingKind::Castle)
        .unwrap()
        .id;
    world
        .issue_train(SEATS[0], castle, UnitKind::Trebuchet)
        .unwrap();
    for _ in 0..120 {
        world.advance();
    }
    let treb = world.units().find(|unit| unit.kind.is_trebuchet()).unwrap();
    assert_eq!(treb.kind, UnitKind::Trebuchet);
    let id = treb.id;
    let target = world.player_buildings(SEATS[1]).next().unwrap().id;
    world.issue_attack(SEATS[0], id, target).unwrap();
    for _ in 0..120 {
        world.advance();
    }
    assert_eq!(world.unit(id).unwrap().kind, UnitKind::TrebuchetUnpacked);
}
