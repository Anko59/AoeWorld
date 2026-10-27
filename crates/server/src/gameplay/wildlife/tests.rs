use crate::gameplay::GameplayService;
use aoe_core::{PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::GameplayServerMessage;
use aoe_simulation::{AnimalKind, GameWorld};

#[tokio::test]
async fn subscription_and_ticks_never_disclose_an_unseen_wolf_or_enemy() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let scout = world.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    let villager = world.spawn_villager(PlayerId(0), at(21, 20)).unwrap();
    let enemy = world.spawn_unit(PlayerId(1), at(100, 100)).unwrap();
    let wolf = world.spawn_animal(AnimalKind::Wolf, at(100, 101)).unwrap();
    let service = GameplayService::from_world(world, scout, None, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(id, 1, TileRect::from_xywh(0, 0, 256, 256))
        .await;
    service.queue_hunt(id, 1, vec![villager], wolf).await;
    service.queue_attack(id, 2, scout, enemy).await;
    service.queue_attack(id, 3, scout, wolf).await;
    service.tick().await;
    let mut checked = 0;
    let mut refused = 0;
    while let Ok(message) = rx.try_recv() {
        match message {
            GameplayServerMessage::Snapshot { units, .. } => {
                assert!(!units.iter().any(|u| u.id == enemy));
                checked += 1;
            }
            GameplayServerMessage::Tick { changed_units, .. } => {
                assert!(!changed_units.iter().any(|u| u.id == enemy));
                checked += 1;
            }
            GameplayServerMessage::WildlifeState { animals, .. } => {
                assert!(!animals.iter().any(|a| a.id == wolf));
                checked += 1;
            }
            GameplayServerMessage::CommandAck { result, .. } => {
                assert_eq!(result, aoe_protocol::CommandResult::RejectedInvalidTarget);
                refused += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 3);
    assert_eq!(refused, 3, "guessed hidden identities cannot be targeted");
}

#[tokio::test]
async fn attack_orders_accept_a_visible_animal() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let scout = world.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    let wolf = world.spawn_animal(AnimalKind::Wolf, at(22, 20)).unwrap();
    let service = GameplayService::from_world(world, scout, None, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(id, 1, TileRect::from_xywh(0, 0, 128, 128))
        .await;
    service.queue_attack(id, 1, scout, wolf).await;
    service.tick().await;
    let mut accepted = false;
    while let Ok(message) = rx.try_recv() {
        if let GameplayServerMessage::CommandAck {
            sequence: 1,
            result,
            ..
        } = message
        {
            assert_eq!(result, aoe_protocol::CommandResult::Accepted);
            accepted = true;
        }
    }
    assert!(accepted, "a visible animal attack must be acknowledged");
}

#[tokio::test]
async fn leaving_sight_removes_enemy_and_wolf_and_resync_does_not_restore_them() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let scout = world.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    let enemy = world.spawn_unit(PlayerId(1), at(22, 20)).unwrap();
    let wolf = world.spawn_animal(AnimalKind::Wolf, at(22, 21)).unwrap();
    world
        .issue_stance(PlayerId(0), &[scout], aoe_core::CombatStance::NoAttack)
        .unwrap();
    world
        .issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let service = GameplayService::from_world(world, scout, None, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let (id, _) = service.register(None, tx).await;
    service
        .subscribe(id, 1, TileRect::from_xywh(0, 0, 128, 128))
        .await;
    let mut saw_wolf = false;
    while let Ok(m) = rx.try_recv() {
        if let GameplayServerMessage::WildlifeState { animals, .. } = m {
            saw_wolf |= animals.iter().any(|a| a.id == wolf);
        }
    }
    assert!(saw_wolf);
    service.queue_move(id, 1, scout, at(60, 20)).await;
    let mut removed = false;
    let mut hidden_wolf = false;
    for _ in 0..400 {
        service.tick().await;
        while let Ok(m) = rx.try_recv() {
            match m {
                GameplayServerMessage::Tick { removals, .. } => {
                    removed |= removals.contains(&enemy)
                }
                GameplayServerMessage::WildlifeState { animals, .. } => {
                    hidden_wolf |= !animals.iter().any(|a| a.id == wolf)
                }
                _ => {}
            }
        }
    }
    assert!(removed && hidden_wolf);
    service.resync(id, 2).await;
    while let Ok(m) = rx.try_recv() {
        match m {
            GameplayServerMessage::Snapshot { units, .. } => {
                assert!(!units.iter().any(|u| u.id == enemy))
            }
            GameplayServerMessage::WildlifeState { animals, .. } => {
                assert!(!animals.iter().any(|a| a.id == wolf))
            }
            _ => {}
        }
    }
}
