use super::*;

#[test]
fn shift_bridge_appends_after_existing_foundations_and_completes_in_order() {
    fn run() -> [u8; 32] {
        let (mut world, worker) = setup();
        fixture::survey(&mut world, PlayerId(0), TileCoord::new(16, 20), 6);
        let first = world
            .issue_build(
                PlayerId(0),
                &[worker],
                BuildingKind::House,
                TileCoord::new(14, 20),
                false,
            )
            .unwrap();
        let second = world
            .issue_build(
                PlayerId(0),
                &[worker],
                BuildingKind::House,
                TileCoord::new(17, 20),
                true,
            )
            .unwrap();
        let bridge = world
            .issue_build_bridge(
                PlayerId(0),
                &[worker],
                TileCoord::new(29, 20),
                TileCoord::new(34, 20),
                true,
            )
            .unwrap();
        assert_eq!(world.economy.builders[&worker].building, first);
        assert_eq!(world.economy.builders[&worker].queue, [second, bridge]);
        for _ in 0..6000 {
            world.advance();
            if !world.building(first).unwrap().is_complete() {
                assert_eq!(world.building(second).unwrap().work_done, 0);
            }
            if !world.building(second).unwrap().is_complete() {
                assert_eq!(world.building(bridge).unwrap().work_done, 0);
            }
            if world.building(bridge).unwrap().is_complete() {
                return world.canonical_hash();
            }
        }
        panic!("queued bridge did not complete");
    }
    assert_eq!(run(), run());
}

#[test]
fn rejected_shift_bridge_keeps_full_construction_queue_and_resources() {
    let (mut world, worker) = setup();
    fixture::survey(&mut world, PlayerId(0), TileCoord::new(14, 20), 3);
    let house = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::House,
            TileCoord::new(14, 20),
            false,
        )
        .unwrap();
    world.economy.builders.get_mut(&worker).unwrap().queue = vec![house; 32];
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 20),
            true
        ),
        Err(EconomyError::OrderTooLarge)
    );
    assert_eq!(world.canonical_hash(), before);
}
