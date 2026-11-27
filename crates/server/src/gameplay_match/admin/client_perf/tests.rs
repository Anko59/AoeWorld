use super::*;
fn report() -> Report {
    let q = [0.0, 1.0, 2.0];
    Report {
        frames: 120,
        frame_ms: [16.0, 32.0, 80.0],
        units_in_view: 12,
        still: Cadence {
            frames: 120,
            max_ms: 80.0,
            over_50_ms: 1,
            over_100_ms: 0,
            histogram: vec![(16, 118), (32, 1), (80, 1)],
        },
        panning: Cadence {
            frames: 0,
            max_ms: 0.0,
            over_50_ms: 0,
            over_100_ms: 0,
            histogram: vec![],
        },
        draw_calls: q,
        renderer: "webgl2".into(),
        contour: Contour {
            effect_frames: 30,
            plain_frames: 90,
            eligible_instances: 12,
            occluder_instances: 40,
            depth_span: 180.5,
            context_lost: 1,
            context_restored: 1,
        },
        stages_ms: Stages {
            update: q,
            interpolation: q,
            culling: q,
            terrain: q,
            sprites: q,
            ui: q,
            fog: q,
        },
    }
}
#[test]
fn nonfinite_negative_and_unordered_observations_are_rejected() {
    for q in [
        [f64::NAN, 1.0, 2.0],
        [0.0, 1.0, f64::INFINITY],
        [-1.0, 1.0, 2.0],
        [2.0, 1.0, 3.0],
        [0.0, 1.0, 600_001.0],
    ] {
        let mut sample = report();
        sample.stages_ms.terrain = q;
        assert!(!sample.valid());
    }
    assert!(report().valid());
}

#[tokio::test]
async fn all_65_human_seats_reach_the_match_journal() {
    use crate::{Config, GameplayService, gameplay::SlotSetup};
    use aoe_core::{EntityId, PlayerId};

    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 10,
        asset_pack: None,
        map_package_directory: Some(dir.path().to_path_buf()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: dir.path().join("geodata"),
    };
    let state = AppState::new(&config, "test").unwrap();
    let (world, _) = aoe_simulation::GameWorld::default_with_village(config.scenario.seed);
    let gameplay = GameplayService::from_world_slots(
        world,
        (1..=65)
            .map(|id| SlotSetup {
                player: PlayerId(id),
                primary_unit_id: EntityId(0),
                human: true,
            })
            .collect(),
        None,
        None,
    );
    let seats = gameplay.slot_infos().await;
    let world_id = gameplay.world_id();
    *state.gameplay.write().await = gameplay;
    let path = dir
        .path()
        .join(format!("matches/{world_id:016x}/telemetry.jsonl"));

    for minute in 0..2 {
        for (index, seat) in seats.iter().enumerate() {
            let mut headers = HeaderMap::new();
            let token: String = seat
                .token
                .0
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            headers.insert("x-aoeworld-controller-token", token.parse().unwrap());
            let mut sample = report();
            sample.units_in_view = minute * 65 + index as u32;
            assert_eq!(
                receive(State(state.clone()), headers.clone(), Json(sample.clone())).await,
                Ok(StatusCode::NO_CONTENT),
                "minute {minute}, seat {index}"
            );
            assert_eq!(
                receive(State(state.clone()), headers, Json(sample)).await,
                Err(StatusCode::TOO_MANY_REQUESTS)
            );
        }
        for entry in state.client_perf.lock().await.by_session.values_mut() {
            entry.at -= Duration::from_secs(60);
        }
    }

    let journal = std::fs::read_to_string(path).unwrap();
    let counts: Vec<_> = journal
        .lines()
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(row["kind"], "client");
            assert!(row.get("token").is_none());
            row["report"]["units_in_view"].as_u64().unwrap()
        })
        .collect();
    assert_eq!(counts, (0..130).collect::<Vec<_>>());
}

#[test]
fn a_seat_keeps_one_report_and_its_own_full_minute_cooldown() {
    let mut reports = Reports::default();
    let token = aoe_protocol::ResumeToken([7; 24]);
    assert_eq!(reports.record(token, report(), 0, 7), Ok(()));
    let entry = reports.by_session.get_mut(&token.0).unwrap();
    entry.at -= Duration::from_secs(56);
    assert_eq!(
        reports.record(token, report(), 0, 7),
        Err(StatusCode::TOO_MANY_REQUESTS)
    );
    let entry = reports.by_session.get_mut(&token.0).unwrap();
    entry.at -= Duration::from_secs(4);
    assert_eq!(reports.record(token, report(), 0, 7), Ok(()));
    assert_eq!(reports.by_session.len(), 1);
    let recent = reports.recent(7);
    assert_eq!(recent.as_array().unwrap().len(), 1);
    assert!(recent[0].get("token").is_none());
    reports.by_session.get_mut(&token.0).unwrap().at -= Duration::from_secs(120);
    assert_eq!(reports.recent(7), serde_json::json!([]));

    let replacement_token = aoe_protocol::ResumeToken([8; 24]);
    assert_eq!(reports.record(replacement_token, report(), 0, 8), Ok(()));
    assert_eq!(reports.by_session.len(), 1);
    assert_eq!(reports.recent(7), serde_json::json!([]));
    assert_eq!(reports.recent(8).as_array().unwrap().len(), 1);
}

#[test]
fn contour_observations_must_be_finite_and_non_negative() {
    for span in [f64::NAN, f64::INFINITY, -1.0] {
        let mut sample = report();
        sample.contour.depth_span = span;
        assert!(!sample.valid());
    }
}
