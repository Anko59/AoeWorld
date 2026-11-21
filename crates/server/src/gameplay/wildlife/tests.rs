use crate::gameplay::GameplayService;
use aoe_core::{PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::GameplayServerMessage;
use aoe_simulation::{AnimalKind, GameWorld};

#[test]
fn an_approaching_wolf_streams_a_valid_walk_before_its_melee_attack() {
    let (mut world, _) =
        GameWorld::behaviour_demo(Seed(7), aoe_simulation::BehaviourScene::Wolf).unwrap();
    let mut approached = false;
    let mut attacked = false;
    for _ in 0..300 {
        world.advance();
        let animals = super::animals(&world, TileRect::from_xywh(0, 0, 256, 256), PlayerId(0));
        for animal in &animals {
            if animal.kind != aoe_protocol::AnimalKind::Wolf {
                continue;
            }
            if animal.moving && animal.task == aoe_protocol::AnimalTask::Idle {
                assert_eq!(animal.target, None, "walking is not an attack pose");
                approached = true;
            }
            attacked |=
                animal.task == aoe_protocol::AnimalTask::Attacking && animal.target.is_some();
        }
        let message = GameplayServerMessage::WildlifeState {
            revision: 1,
            tick: world.tick(),
            animals,
        };
        let bytes = aoe_protocol::encode_gameplay_server(&message).unwrap();
        assert_eq!(
            aoe_protocol::decode_gameplay_server(&bytes).unwrap(),
            message
        );
        if approached && attacked {
            break;
        }
    }
    assert!(approached && attacked, "wolf must walk, then reach melee");
}

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

#[test]
fn a_shooter_is_named_with_its_true_launch_only_while_its_position_is_seen() {
    use aoe_simulation::UnitKind;
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let archer = world
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
        .unwrap();
    let target = world.spawn_villager(PlayerId(1), at(103, 100)).unwrap();
    world.issue_attack(PlayerId(0), archer, target).unwrap();
    world.advance();
    let launched = world.projectiles().next().expect("a shot").launched;
    for _ in 0..3 {
        world.advance();
    }
    let region = TileRect::from_xywh(0, 0, 256, 256);
    // The shooter's own player sees it all: the shooter and its true launch.
    let own = super::projectiles(&world, region, PlayerId(0));
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].attacker, Some(archer));
    assert_eq!(own[0].launched, launched);
    assert_eq!(own[0].from, world.projectiles().next().unwrap().from);
    // A bystander who sees only the flight learns neither shooter nor launch.
    let seen = super::projectiles(&world, region, PlayerId(1));
    for shot in seen {
        let origin_seen = shot.attacker.is_some();
        assert_eq!(shot.launched == launched, origin_seen);
        assert!(origin_seen || shot.launched == world.tick());
    }
}

#[test]
fn siege_and_naval_shots_publish_the_primary_dat_projectile() {
    use aoe_protocol::ProjectileKind;
    use aoe_simulation::UnitKind;
    for (kind, distance, expected) in [
        (UnitKind::HandCannoneer, 5, ProjectileKind::Native(380)),
        (UnitKind::TrebuchetUnpacked, 10, ProjectileKind::Native(371)),
        (UnitKind::BombardCannon, 8, ProjectileKind::Native(368)),
        (UnitKind::Scorpion, 4, ProjectileKind::Native(367)),
        (UnitKind::HeavyScorpion, 4, ProjectileKind::Native(627)),
        (UnitKind::Galley, 4, ProjectileKind::Native(512)),
        (UnitKind::WarGalley, 4, ProjectileKind::Native(372)),
        (UnitKind::Galleon, 4, ProjectileKind::Native(373)),
        (UnitKind::FireShip, 2, ProjectileKind::Native(676)),
    ] {
        let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap();
        let (mut world, target, launch) = if kind.is_naval() {
            let (package, elevation, water) = GameWorld::naval_battle_environment(Seed(7)).unwrap();
            let mut world = GameWorld::from_prepared_map(package, elevation, water, vec![], vec![])
                .unwrap()
                .without_loose_rocks();
            world.prepare_content_roster(0, true).unwrap();
            let ship = world
                .units()
                .find(|u| u.player == PlayerId(1) && u.kind == UnitKind::TransportShip)
                .unwrap();
            let launch = WorldPosition::new(
                ship.position.x - distance * aoe_core::FIXED_SUBUNITS_PER_TILE,
                ship.position.y,
            );
            (world, ship.id, launch)
        } else {
            let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
            let target = world
                .spawn_villager(PlayerId(1), at(100 + distance))
                .unwrap();
            (world, target, at(100))
        };
        world.set_fog_disabled(true);
        let shooter = world.spawn_kind(PlayerId(0), kind, launch).unwrap();
        world.issue_attack(PlayerId(0), shooter, target).unwrap();
        for _ in 0..40 {
            world.advance();
            if world.projectiles().any(|p| p.attacker == shooter) {
                break;
            }
        }
        let config = world.config();
        let shots = super::projectiles(
            &world,
            TileRect::from_xywh(0, 0, config.width_tiles, config.height_tiles),
            PlayerId(0),
        );
        assert_eq!(shots.len(), 1, "{kind:?}");
        assert_eq!(
            (shots[0].attacker, shots[0].kind),
            (Some(shooter), expected)
        );
    }
}

#[test]
fn mangonel_volley_publishes_primary_and_secondary_dat_projectiles() {
    use aoe_protocol::ProjectileKind::Native;
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap();
    let shooter = world
        .spawn_kind(PlayerId(0), aoe_simulation::UnitKind::Mangonel, at(100))
        .unwrap();
    let target = world.spawn_villager(PlayerId(1), at(106)).unwrap();
    world.issue_attack(PlayerId(0), shooter, target).unwrap();
    world.advance();
    let shots = super::projectiles(&world, TileRect::from_xywh(0, 0, 256, 256), PlayerId(0));
    assert_eq!(
        shots.iter().map(|shot| shot.kind).collect::<Vec<_>>(),
        [
            Native(656),
            Native(369),
            Native(369),
            Native(369),
            Native(369),
            Native(369)
        ]
    );
    assert!(shots.iter().all(|shot| shot.attacker == Some(shooter)));
}
