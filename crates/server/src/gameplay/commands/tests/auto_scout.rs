use super::*;
use aoe_core::Seed;

#[test]
fn enabling_auto_scout_takes_a_generals_scout_out_of_his_army() {
    use aoe_core::{PlayerId, TileCoord};
    use aoe_simulation::{GameWorld, UnitKind};
    let mut world = GameWorld::new(aoe_core::WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap();
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(100))
        .unwrap();
    let scout = world
        .spawn_kind(PlayerId(0), UnitKind::Scout, at(102))
        .unwrap();
    for _ in 0..30 {
        world.advance();
    }
    assert_eq!(world.general_controller(scout), Some(general));
    let toggle = PendingAction::AutoScout {
        entity_id: scout,
        enabled: true,
    };
    assert_eq!(
        apply(&mut world, PlayerId(0), toggle),
        CommandResult::Accepted
    );
    assert_eq!(world.general_controller(scout), None);
    // The player now marches the General's army through the same command path.
    let march = PendingAction::Move {
        entity_id: general,
        destination: at(140),
        queued: false,
        attack_move: false,
    };
    assert_eq!(
        apply(&mut world, PlayerId(0), march),
        CommandResult::Accepted
    );
    // Run past the 300-tick idle delay so a readmission would show.
    for _ in 0..400 {
        world.advance();
    }
    assert!(
        world.unit(general).unwrap().position.x > at(120).x,
        "the General did not march"
    );
    assert_eq!(
        world.general_controller(scout),
        None,
        "the march took the scout back into the army"
    );
    assert!(
        world.unit(scout).unwrap().auto_scout,
        "the General's march did not cancel the scouting"
    );
}
