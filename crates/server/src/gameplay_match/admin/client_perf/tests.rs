use super::*;
fn report() -> Report {
    let q = [0.0, 1.0, 2.0];
    Report {
        sequence: (1, 0),
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

    let first_arrival = Instant::now();
    // Reports sent 60.010 s apart with 100 ms then 10 ms transit.
    let arrival_gap = Duration::from_millis(60_010 + 10 - 100);
    for minute in 0_u32..2 {
        let arrival = first_arrival + minute * arrival_gap;
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
            sample.sequence.0 += u64::from(minute) * 60_010_000;
            sample.units_in_view = minute * 65 + index as u32;
            assert_eq!(
                receive_at(state.clone(), headers.clone(), sample.clone(), arrival).await,
                Ok(StatusCode::NO_CONTENT),
                "minute {minute}, seat {index}"
            );
            assert_eq!(
                receive_at(state.clone(), headers, sample, arrival).await,
                Err(StatusCode::CONFLICT)
            );
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
fn burst_budget_and_deduplication_are_independent_per_seat() {
    let mut reports = Reports::default();
    let first = aoe_protocol::ResumeToken([7; 24]);
    let second = aoe_protocol::ResumeToken([8; 24]);
    let now = Instant::now();
    // An older distinct report can arrive after a newer one, including from
    // another page on the seat. Each report gets at most one admission.
    for sequence in [2, 1, 3, 4] {
        let mut sample = report();
        sample.sequence.0 = sequence;
        assert_eq!(reports.admit(first, sample.sequence, 7, now), Ok(()));
        assert_eq!(
            reports.admit(first, sample.sequence, 7, now),
            Err(StatusCode::TOO_MANY_REQUESTS)
        );
        reports.record(first, sample, 0, now);
    }
    for sequence in 5..100 {
        assert_eq!(
            reports.admit(first, (sequence, 0), 7, now + Duration::from_secs(59)),
            Err(StatusCode::TOO_MANY_REQUESTS)
        );
    }
    assert_eq!(reports.admit(second, (2, 0), 7, now), Ok(()));
    let later = now + Duration::from_secs(120);
    assert_eq!(
        reports.admit(first, (1, 0), 7, later),
        Err(StatusCode::CONFLICT)
    );
    assert_eq!(reports.admit(first, (5, 0), 7, later), Ok(()));
    assert_eq!(reports.recent(7).as_array().unwrap().len(), 1);
    reports
        .by_session
        .get_mut(&first.0)
        .unwrap()
        .latest
        .as_mut()
        .unwrap()
        .at -= Duration::from_secs(120);
    assert_eq!(reports.recent(7), serde_json::json!([]));

    assert_eq!(reports.admit(first, (1, 0), 8, later), Ok(()));
    assert_eq!(reports.by_session.len(), 1);
    assert_eq!(reports.recent(7), serde_json::json!([]));
}

#[test]
fn contour_observations_must_be_finite_and_non_negative() {
    for span in [f64::NAN, f64::INFINITY, -1.0] {
        let mut sample = report();
        sample.contour.depth_span = span;
        assert!(!sample.valid());
    }
}
