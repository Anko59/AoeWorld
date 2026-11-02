use super::*;
use aoe_core::{PlayerId, Seed};

#[test]
fn delete_orders_are_authorized_per_player_and_remove_only_owned_entities() {
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(7));
    let own = world
        .units()
        .find(|u| u.kind == aoe_simulation::UnitKind::Villager)
        .unwrap()
        .id;
    let at = world.unit(own).unwrap().position;
    let theirs = world.spawn_villager(PlayerId(1), at).unwrap();
    let delete = |ids: Vec<EntityId>| PendingAction::Delete { entity_ids: ids };
    assert_eq!(
        apply(&mut world, PlayerId(1), delete(vec![own])),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(&mut world, PlayerId(0), delete(vec![own, theirs])),
        CommandResult::RejectedNotOwner,
        "one foreign id refuses the whole selection"
    );
    assert!(world.unit(own).unwrap().hp > 0);
    assert!(world.unit(theirs).unwrap().hp > 0);
    assert_eq!(
        apply(&mut world, PlayerId(0), delete(vec![theirs])),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(&mut world, PlayerId(0), delete(vec![own])),
        CommandResult::Accepted
    );
    assert_eq!(world.unit(own).unwrap().hp, 0);
    assert!(world.unit(theirs).unwrap().hp > 0);
}
