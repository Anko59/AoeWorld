use super::*;
fn report() -> Report {
    let q = [0.0, 1.0, 2.0];
    Report {
        frames: 120,
        frame_ms: [16.0, 32.0, 80.0],
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
fn history_and_expiry_are_bounded_without_exposing_tokens() {
    let mut reports = Reports::default();
    for id in 0..70 {
        if id >= 64 {
            reports.0.front_mut().unwrap().at = Instant::now() - Duration::from_secs(3);
        }
        reports
            .record(aoe_protocol::ResumeToken([id; 24]), report(), 0)
            .unwrap();
    }
    assert_eq!(reports.0.len(), 64);
    assert_eq!(reports.0.front().unwrap().token.0[0], 6);
    reports.0[0].at = Instant::now() - Duration::from_secs(31);
    assert_eq!(reports.recent().as_array().unwrap().len(), 63);
    assert!(reports.recent()[0].get("token").is_none());
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

#[test]
fn cycling_more_than_the_history_limit_cannot_bypass_report_rate_limits() {
    let mut reports = Reports::default();
    for id in 0..64 {
        reports
            .record(aoe_protocol::ResumeToken([id; 24]), report(), 0)
            .unwrap();
    }
    assert_eq!(
        reports.record(aoe_protocol::ResumeToken([64; 24]), report(), 0),
        Err(StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(
        reports.record(aoe_protocol::ResumeToken([0; 24]), report(), 0),
        Err(StatusCode::TOO_MANY_REQUESTS)
    );
}

#[test]
fn contour_observations_must_be_finite_and_non_negative() {
    for span in [f64::NAN, f64::INFINITY, -1.0] {
        let mut sample = report();
        sample.contour.depth_span = span;
        assert!(!sample.valid());
    }
}
