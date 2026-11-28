use super::{FIRST, SECOND, at, world};
use crate::{BuildingKind, DamageKind};
use aoe_core::{PlayerId, TileCoord};

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
