use crate::GameplayService;
use aoe_core::{Seed, WorldPosition};

#[tokio::test]
async fn saved_recording_waits_for_ai_without_resuming_an_ordinary_paused_match() {
    let directory = tempfile::tempdir().unwrap();
    let service = GameplayService::new(Seed(3));
    service.set_paused(true);
    service.recording_ai_ready();
    assert!(service.paused());
    service.set_paused(false);
    service
        .start_saved_recording(&directory.path().join("commands.jsonl"), 2)
        .await
        .unwrap();
    assert!(service.paused());
    service.recording_ai_ready();
    assert!(!service.paused());
    service.set_paused(true);
    service.recording_ai_ready();
    assert!(
        service.paused(),
        "readiness must resume the recording only once"
    );
}

#[tokio::test]
async fn recorded_applied_commands_reproduce_every_tick_without_sessions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commands.jsonl");
    let live = GameplayService::new(Seed(41));
    let replay = GameplayService::new(Seed(41));
    live.start_recording(&path, 8).await.unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(100);
    let (session, welcome) = live.register(None, sender).await;
    let aoe_protocol::GameplayServerMessage::Welcome {
        primary_unit_id: primary,
        ..
    } = welcome
    else {
        panic!("welcome")
    };
    live.queue_move(session, 1, primary, WorldPosition { x: 40, y: 40 })
        .await;
    live.queue_stance(session, 2, vec![primary], aoe_core::CombatStance::NoAttack)
        .await;
    for index in 0..8 {
        if index == 4 {
            live.queue_stop(session, 3, primary).await;
        }
        live.tick().await;
    }
    let recording = crate::replay::Recording::read(&path).unwrap();
    assert_eq!(recording.ticks.len(), 8);
    assert_eq!(
        recording
            .ticks
            .iter()
            .map(|t| t.commands.len())
            .sum::<usize>(),
        3
    );
    replay
        .inspect_scale_world(|w| assert_eq!(w.canonical_hash(), recording.initial_hash))
        .await;
    for tick in &recording.ticks {
        let hash = replay
            .replay_step_using(tick, &mut |_| {}, || Ok(super::timing::Probe::fixture()))
            .await
            .unwrap();
        assert_eq!(hash, tick.hash, "tick {}", tick.tick);
    }
    assert!(replay.sessions.lock().await.is_empty());
    let decisions = |world: &aoe_simulation::GameWorld| {
        world.order_demand()[aoe_simulation::OrderIssuer::Player as usize].decisions
    };
    assert_eq!(live.inspect_scale_world(decisions).await, 3);
    assert_eq!(replay.inspect_scale_world(decisions).await, 3);
    let mut omitted: crate::replay::RecordedTick =
        serde_json::from_value(serde_json::to_value(&recording.ticks[0]).unwrap()).unwrap();
    omitted.commands.clear();
    let missing_input = GameplayService::new(Seed(41));
    assert_ne!(
        missing_input
            .replay_step_using(
                &omitted,
                &mut |_| {},
                || Ok(super::timing::Probe::fixture())
            )
            .await
            .unwrap(),
        recording.ticks[0].hash
    );
}

#[tokio::test]
async fn incomplete_recording_and_overwrite_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commands.jsonl");
    let service = GameplayService::new(Seed(3));
    service.start_recording(&path, 2).await.unwrap();
    assert!(service.start_recording(&path, 2).await.is_err());
    assert!(
        GameplayService::new(Seed(3))
            .start_recording(&path, 2)
            .await
            .is_err()
    );
    service.tick().await;
    assert!(crate::replay::Recording::read(&path).is_err());
    service.tick().await;
    assert_eq!(
        crate::replay::Recording::read(&path).unwrap().ticks.len(),
        2
    );
}

#[tokio::test]
async fn replay_refuses_a_tick_gap_before_applying_commands() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commands.jsonl");
    let live = GameplayService::new(Seed(19));
    live.start_recording(&path, 2).await.unwrap();
    live.tick().await;
    live.tick().await;
    let recording = crate::replay::Recording::read(&path).unwrap();
    let replay = GameplayService::new(Seed(19));
    assert!(
        replay
            .replay_step_using(&recording.ticks[1], &mut |_| {}, || Ok(
                super::timing::Probe::fixture()
            ))
            .await
            .is_err()
    );
    assert_eq!(replay.inspect_scale_world(|w| w.tick().0).await, 0);
}
