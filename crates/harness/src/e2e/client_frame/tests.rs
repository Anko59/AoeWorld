use super::*;

fn capture() -> Capture {
    Capture {
        counters: vec!["terrain.tiles".into()],
        frames: vec![vec![3], vec![4]],
        wasm_sha256: "wasm".into(),
        bridge_sha256: "bridge".into(),
        chunks_sha256: "chunks".into(),
        video: None,
    }
}

#[test]
fn capture_validation_rejects_unlabelled_work_duplicate_names_and_changed_inputs() {
    assert!(validate_captures(&[], 2).is_err());
    assert!(validate_captures(&[capture()], 2).is_ok());
    assert!(validate_captures(&[capture()], 3).is_err());
    let mut invalid = capture();
    invalid.frames[0].push(7);
    assert!(validate_captures(&[invalid], 2).is_err());
    let mut invalid = capture();
    invalid.counters.push("terrain.tiles".into());
    invalid.frames = vec![vec![3, 4], vec![3, 4]];
    assert!(validate_captures(&[invalid], 2).is_err());
    let mut changed = capture();
    changed.chunks_sha256 = "different map inputs".into();
    assert!(validate_captures(&[capture(), changed], 2).is_err());
}

#[test]
fn fixed_script_preserves_idle_pan_zoom_and_return_frames() {
    let frames = script(512).unwrap();
    assert_eq!(frames.len(), 48);
    assert_eq!(frames[0], frames[7]);
    assert_ne!(frames[7], frames[8]);
    assert_eq!(frames[0], frames[40]);
    assert!(frames.iter().any(|frame| frame[2] == 0.25));
    assert!(frames.iter().any(|frame| frame[2] == 1.0));
    assert!(script(0).is_err());
}

#[test]
fn repeatability_uses_every_integer_counter_and_rejects_missing_frames() {
    let run = vec![vec![10, 200], vec![30, 400]];
    let summary = summarize(&[run.clone(), run.clone(), run]).unwrap();
    assert_eq!(summary.means, vec![20.0, 300.0]);
    assert_eq!(summary.spread_percent, vec![0.0, 0.0]);
    assert!(summarize(&[]).is_err());
    assert!(summarize(&[vec![]]).is_err());
    assert!(summarize(&[vec![vec![1]], vec![vec![1], vec![1]]]).is_err());
    assert!(summarize(&[vec![vec![1], vec![1, 2]]]).is_err());
    let changed = summarize(&[vec![vec![100]], vec![vec![101]], vec![vec![102]]]).unwrap();
    assert!((changed.spread_percent[0] - 200.0 / 101.0).abs() < 1e-9);
    let moved = summarize(&[vec![vec![0], vec![10]], vec![vec![10], vec![0]]]).unwrap();
    assert_eq!(moved.spread_percent, [0.0]);
    assert_eq!(moved.max_frame_spread_percent, [200.0]);
}
