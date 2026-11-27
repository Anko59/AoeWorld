use super::*;
use serde_json::{Value, json};

fn server(at: u64, ticks: u32, units: usize) -> Value {
    json!({
        "kind": "server", "elapsed_ms": at, "window_ms": 60_000,
        "tick": 1000, "ticks": ticks, "ticks_per_second": f64::from(ticks) / 60.0,
        "units": units, "phases_us": {}, "rust_ai": null
    })
}

fn client(at: u64, frames: u32, bin: u8, max: f64, panning: bool) -> Value {
    let zero = json!([0, 0, 0]);
    let empty =
        json!({"frames": 0, "max_ms": 0, "over_50_ms": 0, "over_100_ms": 0, "histogram": []});
    let cadence = json!({"frames": frames, "max_ms": max, "histogram": [[bin, frames]],
        "over_50_ms": if bin > 50 { frames } else { 0 },
        "over_100_ms": if bin > 100 { frames } else { 0 }});
    let (still, panning) = if panning {
        (empty, cadence)
    } else {
        (cadence, empty)
    };
    json!({
        "kind": "client", "elapsed_ms": at, "tick": 1000,
        "report": {
            "sequence": [at, 1], "frames": frames, "frame_ms": [max, max, max], "renderer": "webgl2",
            "units_in_view": 250, "still": still, "panning": panning, "draw_calls": zero,
            "stages_ms": {
                "update": zero, "interpolation": zero, "culling": zero, "terrain": zero,
                "sprites": zero, "ui": zero, "fog": zero
            },
            "contour": {
                "effect_frames": 0, "plain_frames": 0, "eligible_instances": 0,
                "occluder_instances": 0, "depth_span": 0, "context_lost": 0, "context_restored": 0
            }
        }
    })
}

#[test]
fn windows_pool_frame_ranks_and_rates_and_preserve_camera_motion() {
    let records = [
        server(600_000, 240, 90_000),
        client(600_001, 1, 17, 16.9, false),
        client(120_000, 2, 137, 1100.0, false),
        client(600_000, 3, 102, 133.0, true),
        server(60_000, 480, 70_000),
        client(60_000, 300, 17, 16.9, false),
    ];
    let text = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let output = serde_json::to_value(summarize(text.as_bytes()).unwrap()).unwrap();
    assert_eq!(output[0]["ticks_per_second"], 6.0);
    assert_eq!(output[0]["server_seconds"], 120.0);
    assert_eq!(output[0]["units"], 90_000);
    assert_eq!(
        output[0]["still"],
        json!({
            "frames": 302, "p99_ms": 17.0, "max_ms": 1100.0, "over_50_ms": 2, "over_100_ms": 2
        })
    );
    assert_eq!(output[0]["panning"]["p99_ms"], 133.0);
    assert_eq!(output[0]["panning"]["over_100_ms"], 3);
    assert_eq!(output[1]["start_minute"], 10);
    assert_eq!(output[1]["ticks_per_second"], Value::Null);
    assert_eq!(output[1]["panning"]["p99_ms"], Value::Null);
    assert_eq!(output[1]["still"]["p99_ms"], 16.9);
}

#[test]
fn invalid_records_are_rejected_with_line_number() {
    let valid = server(60_000, 240, 10);
    let text = format!("{valid}\n{{");
    let error = summarize(text.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("line 2"), "{error}");
    let mut invalid = client(60_000, 1, 17, 16.9, true);
    invalid["report"]["panning"]["histogram"][0][0] = json!(255);
    let error = summarize(invalid.to_string().as_bytes()).unwrap_err();
    assert!(error.to_string().contains("line 1"), "{error}");
}
