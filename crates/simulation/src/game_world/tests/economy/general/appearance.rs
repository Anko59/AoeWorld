use super::*;

#[test]
fn appearance_demo_uses_normal_clock_and_replays_idle_walk_attack_and_death() {
    fn run(fog_disabled: bool) -> Vec<[u8; 32]> {
        let (mut world, general) = GameWorld::general_appearance_demo(aoe_core::Seed(1)).unwrap();
        world.set_fog_disabled(fog_disabled);
        assert_eq!(world.config.tick_hz, 20);
        assert_eq!(world.unit(general).unwrap().kind, UnitKind::General);
        advance(&mut world, 40);
        assert_eq!(world.unit(general).unwrap().task, crate::UnitTask::Idle);
        let target = world
            .units()
            .find(|u| u.player == PlayerId(1) && u.kind == UnitKind::BatteringRam)
            .unwrap()
            .id;
        world.issue_attack(PlayerId(0), general, target).unwrap();
        let (mut moved, mut attacked, mut died) = (false, false, false);
        let mut hashes = Vec::new();
        for _ in 0..1200 {
            world.advance();
            if let Some(unit) = world.unit(general) {
                moved |= unit.moving;
                died |= unit.hp == 0;
            }
            attacked |= world
                .unit(target)
                .and_then(|u| u.last_hit)
                .is_some_and(|hit| hit.attacker == general);
            hashes.push(world.canonical_hash());
        }
        assert!(
            moved && attacked && died,
            "movement={moved}, attack={attacked}, death={died}, hero={:?}",
            world.unit(general)
        );
        hashes
    }
    for fog_disabled in [false, true] {
        assert_eq!(run(fog_disabled), run(fog_disabled));
    }
}
