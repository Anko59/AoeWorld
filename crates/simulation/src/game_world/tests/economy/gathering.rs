use super::*;

#[test]
fn each_resource_uses_its_exact_rate_and_the_ten_unit_carry_cap() {
    for (kind, hundredths) in [
        (ResourceKind::Wood, 39),
        (ResourceKind::Food, 31),
        (ResourceKind::Gold, 38),
        (ResourceKind::Stone, 36),
    ] {
        let (mut world, node, id) = resource_world(kind);
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        advance(&mut world, 20 * 10);
        let unit = world.unit(id).unwrap();
        assert_eq!(
            unit.carried,
            Some(CarriedResource {
                kind,
                amount: hundredths / 10
            })
        );
        assert_eq!(
            world.resource_remaining(node.id).unwrap(),
            node.initial_amount - hundredths / 10
        );
        advance(&mut world, 20 * 40);
        assert_eq!(world.unit(id).unwrap().carried.unwrap().amount, 10);
        assert_eq!(
            world.resource_remaining(node.id).unwrap(),
            node.initial_amount - 10
        );
    }
}

#[test]
fn a_full_load_returns_deposits_and_resumes_the_same_node() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    let before = world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    let mut returning = false;
    let mut deposited = false;
    for _ in 0..6000 {
        world.advance();
        let unit = world.unit(id).unwrap();
        assert!(unit.carried.is_none_or(|load| load.amount <= 10));
        returning |= unit.task == UnitTask::Returning;
        if world.player_economy(PlayerId(0)).unwrap().stockpile.wood == before + 10 {
            deposited = true;
            assert_eq!(world.gather_target(id), Some(node.id));
            break;
        }
    }
    assert!(returning && deposited);
    advance(&mut world, 2000);
    assert!(world.resource_remaining(node.id).unwrap() < node.initial_amount - 10);
}

#[test]
fn depleted_nodes_deposit_partial_load_then_find_only_nearby_same_type() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    world
        .deplete_resource(node.id, node.initial_amount - 2)
        .unwrap();
    let replacement = world
        .nearest_resource(node.tile, node.kind, 10, Some(node.id))
        .unwrap();
    assert_ne!(replacement.id, node.id);
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    let before = world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    for _ in 0..4000 {
        world.advance();
        if world.player_economy(PlayerId(0)).unwrap().stockpile.wood > before {
            break;
        }
    }
    assert_eq!(world.resource_remaining(node.id).unwrap(), 0);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        before + 2
    );
    assert_eq!(world.gather_target(id), Some(replacement.id));
    assert!(
        world
            .resource_snapshot([1; 32])
            .unwrap()
            .changes
            .iter()
            .any(|change| change.id == node.id && change.remaining == 0)
    );
}

#[test]
fn no_nearby_resource_leaves_the_villager_idle_after_deposit() {
    let (mut world, node, id) = resource_world(ResourceKind::Gold);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    while let Some(other) = world.nearest_resource(node.tile, node.kind, 10, Some(node.id)) {
        world
            .deplete_resource(other.id, other.initial_amount)
            .unwrap();
    }
    world
        .deplete_resource(node.id, node.initial_amount - 1)
        .unwrap();
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    advance(&mut world, 6000);
    let unit = world.unit(id).unwrap();
    assert_eq!(unit.task, UnitTask::Idle);
    assert_eq!(unit.carried, None);
    assert_eq!(world.gather_target(id), None);
}

#[test]
fn changing_type_drops_load_and_move_cancels_gathering() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    advance(&mut world, 200);
    assert!(world.unit(id).unwrap().carried.is_some());
    let gold = world
        .nearest_resource(node.tile, ResourceKind::Gold, 10, None)
        .unwrap();
    world.issue_gather(PlayerId(0), id, gold.id).unwrap();
    assert!(world.unit(id).unwrap().carried.is_none());
    let current = world.unit(id).unwrap().position;
    world.issue_move(id, current).unwrap();
    assert_eq!(world.gather_target(id), None);
    let remaining = world.resource_remaining(gold.id).unwrap();
    advance(&mut world, 200);
    assert_eq!(world.resource_remaining(gold.id).unwrap(), remaining);
}

#[test]
fn gather_rejections_preserve_orders_and_carried_load() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    assert_eq!(
        world.issue_gather(PlayerId(1), id, node.id),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        world.issue_gather(PlayerId(0), EntityId(u32::MAX), node.id),
        Err(EconomyError::UnknownEntity)
    );
    let scout = world
        .spawn_unit(PlayerId(0), world.unit(id).unwrap().position)
        .unwrap();
    assert_eq!(
        world.issue_gather(PlayerId(0), scout, node.id),
        Err(EconomyError::WrongUnitKind)
    );
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    advance(&mut world, 200);
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_gather(PlayerId(0), id, u64::MAX),
        Err(EconomyError::InvalidResource)
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn a_failed_resource_sink_never_depletes_or_creates_carried_resources() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    for _ in 0..200 {
        world.advance_with_resource_sink(&mut |_| false);
    }
    assert_eq!(
        world.resource_remaining(node.id).unwrap(),
        node.initial_amount
    );
    assert_eq!(world.unit(id).unwrap().carried, None);
    advance(&mut world, 200);
    assert_eq!(world.unit(id).unwrap().carried.unwrap().amount, 3);
}
