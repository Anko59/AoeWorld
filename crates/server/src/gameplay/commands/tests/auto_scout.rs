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
    world.issue_move(general, at(140)).unwrap();
    for _ in 0..60 {
        world.advance();
    }
    assert!(
        world.unit(scout).unwrap().auto_scout,
        "the General's march did not cancel the scouting"
    );
}
