use super::*;

#[tokio::test]
async fn shared_pages_cannot_expand_the_match_command_storage_budget() {
    let service = GameplayService::new(Seed(7));
    let mut receivers = Vec::new();
    for _ in 0..53 {
        let (sender, receiver) = mpsc::channel(32);
        receivers.push(receiver);
        let (id, _) = service.register(None, sender).await;
        for sequence in 1..=20 {
            service.queue_stop(id, sequence, EntityId(0)).await;
        }
    }
    let sessions = service.sessions.lock().await;
    assert_eq!(total_pending(&sessions), MAX_PENDING_COMMANDS_GLOBAL);
    let rejected = receivers
        .iter_mut()
        .flat_map(|receiver| {
            let mut messages = Vec::new();
            while let Ok(message) = receiver.try_recv() {
                messages.push(message);
            }
            messages
        })
        .filter(|message| {
            matches!(
                message,
                GameplayServerMessage::CommandAck {
                    result: CommandResult::RejectedQueueFull,
                    ..
                }
            )
        })
        .count();
    assert_eq!(rejected, 53 * 20 - MAX_PENDING_COMMANDS_GLOBAL);
}

#[test]
fn general_only_move_is_owner_checked_and_leaves_the_army_in_place() {
    use aoe_core::PlayerId;
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(7));
    let at = |x: i32| WorldPosition::new(x * 1024 + 512, 100 * 1024 + 512);
    let soldier = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::Militia, at(100))
        .unwrap();
    let general = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::General, at(102))
        .unwrap();
    let action = |entity_id| PendingAction::GeneralMoveAlone {
        entity_id,
        destination: at(140),
    };
    assert_eq!(
        apply(&mut world, PlayerId(1), action(general)),
        CommandResult::RejectedNotOwner
    );
    assert!(matches!(
        apply(&mut world, PlayerId(0), action(soldier)),
        CommandResult::RejectedWrongUnitKind
    ));
    assert_eq!(
        apply(&mut world, PlayerId(0), action(general)),
        CommandResult::Accepted
    );
    let start = world.unit(soldier).unwrap().position;
    for _ in 0..300 {
        world.advance();
    }
    assert_eq!(world.unit(soldier).unwrap().position, start);
    assert!(world.unit(general).unwrap().position.x > 120 * 1024);
}

#[test]
fn general_train_and_mix_are_owner_checked_and_gate_recruiting() {
    use aoe_core::PlayerId;
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(7));
    let at = |x: i32| WorldPosition::new(x * 1024 + 512, 100 * 1024 + 512);
    let general = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::General, at(102))
        .unwrap();
    let train = |enabled| PendingAction::GeneralTraining {
        entity_id: general,
        enabled,
    };
    let mix = |mix| PendingAction::GeneralMix {
        entity_id: general,
        mix,
    };
    assert_eq!(
        apply(&mut world, PlayerId(1), train(true)),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(&mut world, PlayerId(0), train(true)),
        CommandResult::Accepted
    );
    assert!(world.general_status(general).unwrap().training);
    assert_eq!(
        apply(&mut world, PlayerId(0), mix([0, 100, 0, 0, 0])),
        CommandResult::Accepted
    );
    assert_eq!(
        world.general_status(general).unwrap().mix,
        [0, 100, 0, 0, 0]
    );
    assert_ne!(
        apply(&mut world, PlayerId(0), mix([101, 0, 0, 0, 0])),
        CommandResult::Accepted
    );
}

#[test]
fn vassal_controls_apply_owned_settings_and_dismiss_and_echo_only_to_owner() {
    use aoe_core::{PlayerId, TileCoord, WorldPosition};
    use aoe_simulation::{GameWorld, UnitKind, VassalSettings};
    let owner = PlayerId(0);
    let (mut world, _) = GameWorld::vassal_territory_demo(Seed(7)).unwrap();
    let vassal_id = world
        .units()
        .find(|u| u.kind == UnitKind::Vassal)
        .unwrap()
        .id;
    let castle = world
        .buildings()
        .find(|b| b.player == owner && b.kind == aoe_simulation::BuildingKind::Castle)
        .unwrap()
        .id;
    world.issue_garrison(owner, vassal_id, castle).unwrap();
    for _ in 0..20 {
        world.advance();
    }
    let settings = VassalSettings {
        weights: [10, 70, 10, 10],
        cap: 75,
        training: true,
    };
    assert_eq!(
        apply(
            &mut world,
            PlayerId(1),
            PendingAction::VassalSettings {
                vassal_id,
                settings
            }
        ),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(
            &mut world,
            owner,
            PendingAction::VassalSettings {
                vassal_id,
                settings
            }
        ),
        CommandResult::Accepted
    );
    let native = world.building(castle).unwrap();
    let own = crate::gameplay::economy::wire_building_state(native, Some(owner));
    let report = own
        .garrison
        .iter()
        .find(|u| u.id == vassal_id)
        .unwrap()
        .vassal
        .unwrap();
    assert_eq!(
        (report.weights, report.cap, report.training),
        (settings.weights, 75, true)
    );
    assert!(
        crate::gameplay::economy::wire_building_state(native, Some(PlayerId(1)))
            .garrison
            .is_empty()
    );
    let worker = world
        .spawn_villager(
            owner,
            WorldPosition::from_tile_center(TileCoord::new(108, 110)).unwrap(),
        )
        .unwrap();
    assert_eq!(
        apply(
            &mut world,
            PlayerId(1),
            PendingAction::VassalDismiss { vassal_id }
        ),
        CommandResult::RejectedNotOwner
    );
    assert!(world.unit(worker).unwrap().hp > 0);
    assert_eq!(
        apply(
            &mut world,
            owner,
            PendingAction::VassalDismiss { vassal_id }
        ),
        CommandResult::Accepted
    );
    assert_eq!(world.unit(worker).unwrap().hp, 0);
}
