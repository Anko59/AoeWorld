use super::*;

#[test]
fn garrison_orders_and_occupant_views_share_ownership_and_fog_rules() {
    use aoe_core::PlayerId;
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(7));
    let center = world.buildings().next().unwrap().id;
    let worker = world
        .units()
        .find(|u| u.kind == aoe_simulation::UnitKind::Villager)
        .unwrap()
        .id;
    assert_eq!(
        apply(
            &mut world,
            PlayerId(1),
            PendingAction::Garrison {
                entity_id: worker,
                building_id: center
            }
        ),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(
            &mut world,
            PlayerId(0),
            PendingAction::Garrison {
                entity_id: worker,
                building_id: center
            }
        ),
        CommandResult::Accepted
    );
    for _ in 0..400 {
        world.advance();
        if world.unit(worker).is_none() {
            break;
        }
    }
    assert!(world.unit(worker).is_none());
    let native = world.building(center).unwrap();
    let own = crate::gameplay::economy::wire_building_state(&native, Some(PlayerId(0)));
    let enemy = crate::gameplay::economy::wire_building_state(&native, Some(PlayerId(1)));
    assert_eq!(own.garrison[0].id, worker);
    assert!(
        enemy.garrison.is_empty(),
        "enemy memory never discloses occupants"
    );
    assert_eq!(
        apply(
            &mut world,
            PlayerId(1),
            PendingAction::Ungarrison {
                building_id: center,
                entity_id: None
            }
        ),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(
            &mut world,
            PlayerId(0),
            PendingAction::Ungarrison {
                building_id: center,
                entity_id: Some(worker)
            }
        ),
        CommandResult::Accepted
    );
    assert!(world.unit(worker).is_some());
}

#[test]
fn garrison_controls_apply_to_the_ordering_players_building() {
    use aoe_core::PlayerId;
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(7));
    let center = world.buildings().next().unwrap().id;
    assert_eq!(
        apply(&mut world, PlayerId(1), PendingAction::TownBell(center)),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(&mut world, PlayerId(0), PendingAction::TownBell(center)),
        CommandResult::Accepted
    );
    for _ in 0..400 {
        world.advance();
        if world.building(center).unwrap().garrison.len() == 3 {
            break;
        }
    }
    assert_eq!(world.building(center).unwrap().garrison.len(), 3);
    assert_eq!(
        apply(&mut world, PlayerId(1), PendingAction::BackToWork(center)),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(&mut world, PlayerId(0), PendingAction::BackToWork(center)),
        CommandResult::Accepted
    );
    assert!(world.building(center).unwrap().garrison.is_empty());
}
