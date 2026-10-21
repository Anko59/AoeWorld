use super::*;
use aoe_core::Seed;
use tokio::sync::mpsc;

#[tokio::test]
async fn queue_move_rejects_unknown_and_non_controller_sessions_then_deduplicates_sequences() {
    let service = GameplayService::new(Seed(1));
    let (controller_tx, mut controller_rx) = mpsc::channel(MAX_ACK_HISTORY + 16);
    let (controller_id, _) = service.register(None, controller_tx).await;

    service
        .queue_move(
            controller_id + 1_000,
            1,
            EntityId(0),
            WorldPosition::default(),
        )
        .await;
    assert!(controller_rx.try_recv().is_err());

    let (spectator_tx, mut spectator_rx) = mpsc::channel(8);
    let (spectator_id, _) = service.register(None, spectator_tx).await;
    service
        .queue_move(spectator_id, 1, EntityId(0), WorldPosition::default())
        .await;
    assert!(matches!(
        spectator_rx.try_recv().expect("spectator rejection"),
        GameplayServerMessage::CommandAck {
            result: CommandResult::RejectedNotController,
            ..
        }
    ));

    service
        .queue_move(controller_id, 1, EntityId(0), WorldPosition::default())
        .await;
    service
        .queue_move(controller_id, 1, EntityId(0), WorldPosition::default())
        .await;
    assert!(matches!(
        controller_rx
            .try_recv()
            .expect("duplicate sequence rejection"),
        GameplayServerMessage::CommandAck {
            sequence: 1,
            result: CommandResult::RejectedSequence,
            ..
        }
    ));

    for sequence in (0..MAX_ACK_HISTORY + 2).rev() {
        service
            .queue_move(
                controller_id,
                sequence as u64,
                EntityId(0),
                WorldPosition::default(),
            )
            .await;
    }
    let mut rejected = 0;
    while let Ok(GameplayServerMessage::CommandAck {
        result: CommandResult::RejectedSequence,
        ..
    }) = controller_rx.try_recv()
    {
        rejected += 1;
    }
    assert!(rejected > MAX_ACK_HISTORY);
}

#[test]
fn stop_command_applies_to_owned_sheep_and_preserves_typed_rejections() {
    use aoe_core::{PlayerId, TileCoord};
    use aoe_simulation::{AnimalKind, GameWorld};
    let mut world = GameWorld::new(aoe_core::WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap();
    world.spawn_villager(PlayerId(0), at(100)).unwrap();
    let sheep = world.spawn_animal(AnimalKind::Sheep, at(101)).unwrap();
    world.advance();
    assert_eq!(
        apply(
            &mut world,
            PlayerId(0),
            PendingAction::Move {
                entity_id: sheep,
                destination: at(110),
            }
        ),
        CommandResult::Accepted
    );
    for _ in 0..7 {
        world.advance();
    }
    let position = world.animal(sheep).unwrap().position;
    assert!(world.animal(sheep).unwrap().moving);
    assert_eq!(
        apply(
            &mut world,
            PlayerId(1),
            PendingAction::Stop { entity_id: sheep }
        ),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(
            &mut world,
            PlayerId(0),
            PendingAction::Stop {
                entity_id: EntityId(u32::MAX)
            }
        ),
        CommandResult::RejectedUnknownEntity
    );
    assert_eq!(
        apply(
            &mut world,
            PlayerId(0),
            PendingAction::Stop { entity_id: sheep }
        ),
        CommandResult::Accepted
    );
    for _ in 0..100 {
        world.advance();
    }
    assert_eq!(world.animal(sheep).unwrap().position, position);
    assert!(!world.animal(sheep).unwrap().moving);
}

#[test]
fn attack_orders_apply_with_typed_rejections_and_reach_the_wire_state() {
    use aoe_core::PlayerId;
    use aoe_protocol::UnitTask;
    let (mut world, scout) = aoe_simulation::GameWorld::default_with_village(Seed(1));
    let position = world.unit(scout).unwrap().position;
    let enemy = world.spawn_militia(PlayerId(1), position).unwrap();
    let attack = |entity_id, target_id| PendingAction::Attack {
        entity_id,
        target_id,
    };
    let own = PlayerId(0);
    assert_eq!(
        apply(&mut world, own, attack(scout, scout)),
        CommandResult::RejectedInvalidTarget
    );
    assert_eq!(
        apply(&mut world, own, attack(EntityId(9_999), enemy)),
        CommandResult::RejectedUnknownEntity
    );
    assert_eq!(
        apply(&mut world, PlayerId(1), attack(scout, enemy)),
        CommandResult::RejectedNotOwner
    );
    assert_eq!(
        apply(&mut world, own, attack(scout, enemy)),
        CommandResult::Accepted
    );
    world.advance();
    let attacker = super::super::economy::unit_state(&world, &world.unit(scout).unwrap());
    assert_eq!(attacker.task, UnitTask::Attacking);
    assert_eq!(attacker.attack_target, Some(enemy));
    let victim = super::super::economy::unit_state(&world, &world.unit(enemy).unwrap());
    assert_eq!((victim.hp, victim.max_hp), (37, 40));
    let hit = victim.last_hit.expect("the hit is reported");
    assert_eq!((hit.attacker, hit.tick), (scout, world.tick()));
}

#[test]
fn a_wall_order_at_the_coordinate_limit_is_rejected_as_invalid_placement() {
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(1));
    let villager = world
        .units()
        .find(|unit| unit.kind == aoe_simulation::UnitKind::Villager)
        .unwrap()
        .id;
    let result = apply(
        &mut world,
        aoe_core::PlayerId(0),
        PendingAction::BuildWall {
            builder_ids: vec![villager],
            from: aoe_core::TileCoord::new(i32::MAX, 3),
            to: aoe_core::TileCoord::new(i32::MAX, 5),
        },
    );
    assert_eq!(result, CommandResult::RejectedInvalidPlacement);
}

#[test]
fn an_attack_without_a_reachable_position_reports_an_unreachable_route() {
    use aoe_core::{PlayerId, TileCoord, WorldConfig};
    use aoe_simulation::GameWorld;
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let attacker = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let target = world.spawn_villager(PlayerId(1), at(106, 100)).unwrap();
    // Four non-overlapping footprints enclose all eight neighboring tiles.
    // Their vision also keeps the target visible to the commanding player.
    for (x, y) in [(96, 97), (100, 96), (101, 100), (97, 101)] {
        world
            .place_town_center(PlayerId(0), TileCoord::new(x, y))
            .unwrap();
    }
    let before = world.canonical_hash();
    assert_eq!(
        apply(
            &mut world,
            PlayerId(0),
            PendingAction::Attack {
                entity_id: attacker,
                target_id: target,
            }
        ),
        CommandResult::RejectedUnreachable,
    );
    assert_eq!(world.canonical_hash(), before);
}
