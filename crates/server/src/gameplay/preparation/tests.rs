use super::*;
use std::sync::atomic::AtomicUsize;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preparation_is_deduplicated_bounded_and_never_blocks_the_runtime() {
    let preparation = Preparation::default();
    let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    struct Release(Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);
    impl Drop for Release {
        fn drop(&mut self) {
            let (lock, changed) = &*self.0;
            *lock.lock().unwrap() = true;
            changed.notify_all();
        }
    }
    let _release_on_failure = Release(release.clone());
    let running = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let mut ready = Vec::new();
    for n in 0..MAX_PENDING {
        let (release, running, maximum) = (release.clone(), running.clone(), maximum.clone());
        ready.push(
            preparation
                .launch((n as i32, 0), move |_| {
                    let count = running.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(count, Ordering::SeqCst);
                    let (lock, changed) = &*release;
                    let mut released = lock.lock().unwrap();
                    while !*released {
                        released = changed.wait(released).unwrap();
                    }
                    running.fetch_sub(1, Ordering::SeqCst);
                })
                .unwrap(),
        );
    }
    let duplicate = preparation
        .launch((0, 0), |_| panic!("duplicate work"))
        .unwrap();
    assert!(Arc::ptr_eq(&duplicate, &ready[0]));
    assert!(preparation.launch((MAX_PENDING as i32, 0), |_| {}).is_err());
    // A blocked source worker must leave async ticks and other sockets runnable.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while running.load(Ordering::SeqCst) < WORKERS {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(ready.iter().all(|flag| !flag.load(Ordering::Acquire)));
    let (lock, changed) = &*release;
    *lock.lock().unwrap() = true;
    changed.notify_all();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while ready.iter().any(|flag| !flag.load(Ordering::Acquire)) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(maximum.load(Ordering::SeqCst), WORKERS);
    assert_eq!(preparation.pending.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn retirement_cancels_queued_preparation_and_rejects_new_jobs() {
    let preparation = Preparation::default();
    let held = preparation
        .workers
        .clone()
        .acquire_many_owned(WORKERS as u32)
        .await
        .unwrap();
    let ready = preparation
        .launch((0, 0), |_| panic!("queued work ran after retirement"))
        .unwrap();
    preparation.close();
    drop(held);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !ready.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(preparation.launch((1, 0), |_| panic!("retired")).is_err());
}

#[tokio::test]
async fn a_cold_order_waits_without_delaying_another_seat_or_the_tick() {
    use crate::gameplay::{GameplayService, SlotSetup};
    use aoe_core::{PlayerId, Seed, WorldConfig, WorldPosition};
    use aoe_protocol::{CommandResult, GameplayServerMessage};
    use aoe_simulation::GameWorld;
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(1)).unwrap()).unwrap();
    let first = world.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    let second = world.spawn_unit(PlayerId(1), at(40, 40)).unwrap();
    let service = GameplayService::from_world_slots(
        world,
        vec![
            SlotSetup {
                player: PlayerId(0),
                primary_unit_id: first,
                human: true,
            },
            SlotSetup {
                player: PlayerId(1),
                primary_unit_id: second,
                human: false,
            },
        ],
        None,
        None,
    );
    let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(64);
    let (tx_b, mut rx_b) = tokio::sync::mpsc::channel(64);
    let (a, _) = service.register(None, tx_a).await;
    let token = service.slot_infos().await[1].token;
    let (b, _) = service.join_slot(token, tx_b).await.unwrap();
    service.queue_move(a, 1, first, at(30, 20)).await;
    let ready = Arc::new(AtomicBool::new(false));
    service
        .sessions
        .lock()
        .await
        .get_mut(&a)
        .unwrap()
        .pending
        .front_mut()
        .unwrap()
        .prepared
        .push(ready.clone());
    service.queue_move(b, 1, second, at(50, 40)).await;
    for _ in 0..4 {
        service.tick().await;
    }
    assert_eq!(service.world.read().await.tick().0, 4);
    assert_eq!(
        service.world.read().await.unit(first).unwrap().position,
        at(20, 20)
    );
    assert_ne!(
        service.world.read().await.unit(second).unwrap().position,
        at(40, 40)
    );
    assert!(rx_a.try_recv().is_err());
    assert!(matches!(
        rx_b.try_recv().unwrap(),
        GameplayServerMessage::CommandAck {
            sequence: 1,
            result: CommandResult::Accepted,
            ..
        }
    ));
    ready.store(true, Ordering::Release);
    service.tick().await;
    assert!(matches!(
        rx_a.try_recv().unwrap(),
        GameplayServerMessage::CommandAck {
            sequence: 1,
            result: CommandResult::Accepted,
            ..
        }
    ));
    assert!(
        service
            .sessions
            .lock()
            .await
            .get(&a)
            .unwrap()
            .pending
            .is_empty()
    );
}

#[tokio::test]
async fn a_join_burst_has_a_hard_tick_command_budget_and_serves_both_seats() {
    use crate::gameplay::{GameplayService, SlotSetup};
    use aoe_core::{PlayerId, Seed, WorldConfig, WorldPosition};
    use aoe_simulation::GameWorld;
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 20)).unwrap();
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(1)).unwrap()).unwrap();
    let first = world.spawn_unit(PlayerId(0), at(20)).unwrap();
    let second = world.spawn_unit(PlayerId(1), at(40)).unwrap();
    let service = GameplayService::from_world_slots(
        world,
        vec![
            SlotSetup {
                player: PlayerId(0),
                primary_unit_id: first,
                human: true,
            },
            SlotSetup {
                player: PlayerId(1),
                primary_unit_id: second,
                human: false,
            },
        ],
        None,
        None,
    );
    let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(128);
    let (tx_b, mut rx_b) = tokio::sync::mpsc::channel(128);
    let (a, _) = service.register(None, tx_a).await;
    let token = service.slot_infos().await[1].token;
    let (b, _) = service.join_slot(token, tx_b).await.unwrap();
    // A newly joined bot starts with 20 tokens; model an earned 40-order
    // burst explicitly so this regression measures tick admission, not refill.
    service
        .sessions
        .lock()
        .await
        .get_mut(&b)
        .unwrap()
        .command_tokens = 40.0;
    for sequence in 1..=18 {
        service.queue_stop(a, sequence, first).await;
    }
    for sequence in 1..=40 {
        service.queue_stop(b, sequence, second).await;
    }
    service.tick().await;
    let mut first_count = 0;
    let mut second_count = 0;
    while rx_a.try_recv().is_ok() {
        first_count += 1;
    }
    while rx_b.try_recv().is_ok() {
        second_count += 1;
    }
    assert_eq!(
        first_count + second_count,
        32,
        "a join burst must leave orders for later ticks"
    );
    assert!(
        first_count > 0 && second_count > 0,
        "one seat monopolized the tick budget"
    );
    service.tick().await;
    while rx_a.try_recv().is_ok() {
        first_count += 1;
    }
    while rx_b.try_recv().is_ok() {
        second_count += 1;
    }
    assert_eq!(
        (first_count, second_count),
        (18, 40),
        "queued verdicts were lost"
    );
}

#[tokio::test]
async fn complete_static_navigation_never_queues_cold_move_preparation() {
    let source = aoe_map::MapChunkGenerator::new([7; 32], 7, 33);
    let loaded = aoe_map::MapChunkGenerator::new([7; 32], 7, 33);
    let mut chunks = Vec::new();
    for y in 0..2 {
        for x in 0..2 {
            chunks.push(
                source
                    .prepare_static_navigation_chunk(x, y, &|| false)
                    .unwrap(),
            );
        }
    }
    loaded.install_static_navigation(chunks).unwrap();
    let preparation = super::Preparation::default();
    let ready = preparation
        .prepare(
            loaded.clone(),
            &[
                aoe_core::TileCoord::new(1, 1),
                aoe_core::TileCoord::new(32, 32),
            ],
        )
        .unwrap();
    assert!(
        ready.is_empty(),
        "a move must be admitted without background readiness flags"
    );
    assert_eq!(loaded.walk_cache_usage(), (0, 0));
    assert!(preparation.pending.lock().unwrap().is_empty());
    preparation.prefetch(loaded.clone(), &[TileCoord::new(1, 1)]);
    assert!(preparation.pending.lock().unwrap().is_empty());
    assert_eq!(loaded.walk_cache_usage(), (0, 0));
}

#[tokio::test]
async fn human_viewport_precedes_queued_background_and_has_reserved_admission() {
    let preparation = Preparation::default();
    let mut held = preparation
        .workers
        .clone()
        .acquire_many_owned(WORKERS as u32)
        .await
        .unwrap();
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut flags = Vec::new();
    for n in 0..MAX_PENDING {
        let order = order.clone();
        flags.push(
            preparation
                .launch((n as i32, 0), move |_| {
                    order.lock().unwrap().push(false);
                })
                .unwrap(),
        );
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let worker = preparation.clone();
    let seen = order.clone();
    let human = tokio::spawn(async move {
        worker
            .terrain(true, move || {
                seen.lock().unwrap().push(true);
                tx.send(()).unwrap();
            })
            .await
            .unwrap();
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while preparation.viewport_slots.available_permits() == MAX_PENDING {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(held.split(1).unwrap());
    tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .unwrap()
        .unwrap();
    human.await.unwrap();
    assert_eq!(order.lock().unwrap().first(), Some(&true));
    drop(held);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while flags.iter().any(|flag| !flag.load(Ordering::Acquire)) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(order.lock().unwrap().len(), MAX_PENDING + 1);
}
