use super::{FIRST, SECOND, at, world};
use crate::{BuildingKind, DamageKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};

#[test]
fn a_conquest_uses_the_current_army_after_another_transfer_in_the_same_tick() {
    let mut world = world();
    let third = PlayerId(2);
    let third_home = world
        .place_town_center(third, TileCoord::new(110, 100))
        .unwrap();
    world.set_diplomacy_ai_seats(&[SECOND, third]);
    let second_home = world
        .player_buildings_of(SECOND, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let attacker = world.spawn_militia(FIRST, at(110, 93)).unwrap();
    let conqueror = world.spawn_militia(third, at(190, 46)).unwrap();
    for (owner, x) in [(SECOND, 195), (third, 110)] {
        for y in 80..83 {
            world.spawn_militia(owner, at(x, y)).unwrap();
        }
    }
    world.resolve_hit(
        (conqueror, third),
        second_home,
        10_000,
        0,
        DamageKind::Melee,
        &mut Vec::new(),
    );
    world.resolve_hit(
        (attacker, FIRST),
        third_home,
        10_000,
        0,
        DamageKind::Melee,
        &mut Vec::new(),
    );
    world.advance();
    assert_eq!(world.surrendered_to(SECOND), Some(third));
    assert_eq!(
        world.surrendered_to(third),
        None,
        "the second candidate just acquired three more soldiers"
    );
}

#[test]
fn destroying_a_replacement_foundation_does_not_steal_conquest_credit() {
    let mut world = world();
    let rival = PlayerId(2);
    world
        .place_town_center(rival, TileCoord::new(110, 100))
        .unwrap();
    world.set_diplomacy_ai_seats(&[SECOND]);
    let center = world
        .player_buildings_of(SECOND, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let attacker = world.spawn_militia(FIRST, at(180, 40)).unwrap();
    let rival_attacker = world.spawn_militia(rival, at(190, 46)).unwrap();
    let defenders: Vec<_> = (0..5)
        .map(|index| world.spawn_militia(SECOND, at(200 + index, 70)).unwrap())
        .collect();
    lethal_hit(&mut world, (attacker, FIRST), center);
    world.advance();
    assert_eq!(world.surrendered_to(SECOND), None);

    let foundation = world
        .insert_building(
            SECOND,
            BuildingKind::TownCenter,
            TileRect::from_xywh(195, 48, 4, 4),
            false,
        )
        .unwrap();
    world.start_foundation(foundation).unwrap();
    lethal_hit(&mut world, (rival_attacker, rival), foundation);
    assert!(world.building(foundation).is_none());
    lethal_hit(&mut world, (attacker, FIRST), defenders[0]);
    world.advance();
    assert_eq!(world.surrendered_to(SECOND), Some(FIRST));
}

#[test]
fn a_neutral_attacker_does_not_earn_conquest_credit() {
    let mut world = world();
    world.set_diplomacy_ai_seats(&[SECOND]);
    world
        .set_diplomatic_stance(FIRST, SECOND, crate::DiplomaticStance::Neutral)
        .unwrap();
    let center = world
        .player_buildings_of(SECOND, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let attacker = world.spawn_militia(FIRST, at(180, 40)).unwrap();
    lethal_hit(&mut world, (attacker, FIRST), center);
    assert!(world.building(center).is_none());
    world.advance();
    assert_eq!(world.surrendered_to(SECOND), None);
}

fn lethal_hit(game: &mut crate::GameWorld, source: (EntityId, PlayerId), target: EntityId) {
    game.resolve_hit(
        source,
        target,
        10_000,
        0,
        DamageKind::Melee,
        &mut Vec::new(),
    );
}
