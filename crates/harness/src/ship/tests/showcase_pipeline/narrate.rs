use super::{Environment, mode};
use crate::ship::{
    showcase::{VoiceLine, narrate_video, parse_lines, probe_seconds, silences},
    tests::showcase_security::ENV_LOCK,
};

fn line(at: f64) -> VoiceLine {
    VoiceLine {
        at,
        text: "x".into(),
    }
}

#[test]
fn lines_are_validated_strictly() {
    let ok = r#"[{"at":0,"text":"One."},{"at":2.5,"text":"Two."}]"#;
    assert_eq!(parse_lines(ok).unwrap().len(), 2);
    for (text, expected) in [
        ("[]", "1 to 40 lines"),
        (r#"[{"at":-1,"text":"a"}]"#, "`at` must be"),
        (r#"[{"at":301,"text":"a"}]"#, "`at` must be"),
        (r#"[{"at":1,"text":"  "}]"#, "text must be"),
        (r#"[{"at":3,"text":"a"},{"at":1,"text":"b"}]"#, "ascending"),
        (r#"[{"at":1,"text":"a","extra":1}]"#, "unknown field"),
        (r#"{"at":1}"#, "narration lines must be"),
    ] {
        let error = parse_lines(text).unwrap_err().to_string();
        assert!(error.contains(expected), "{text}: {error}");
    }
    let long = format!(r#"[{{"at":1,"text":"{}"}}]"#, "a".repeat(3001));
    assert!(parse_lines(&long).is_err());
    let many = format!("[{}]", vec![r#"{"at":1,"text":"a"}"#; 41].join(","));
    assert!(parse_lines(&many).is_err());
}

#[test]
fn silences_place_voices_at_their_timestamps_and_refuse_overlap_and_overrun() {
    // One second of voice is 48 000 bytes at 24 kHz, 16-bit mono.
    let lines = [line(1.0), line(4.0)];
    let gaps = silences(&lines, &[48_000, 24_000], 10.0).unwrap();
    assert_eq!(gaps, vec![48_000, 96_000, 264_000]);
    let overlap = silences(&[line(1.0), line(1.5)], &[48_000, 1_000], 10.0).unwrap_err();
    assert!(overlap.to_string().contains("still speaking"), "{overlap}");
    let past = silences(&[line(9.5)], &[48_000], 10.0).unwrap_err();
    assert!(past.to_string().contains("past the 10.00s video"), "{past}");
}

#[test]
fn probe_reads_duration_and_requires_video() {
    let banner = "Input #0, matroska,webm, from 'x':\n  Duration: 00:01:02.50, start: 0.0\n  Stream #0:0: Video: vp8\n";
    assert_eq!(probe_seconds(banner), Some(62.5));
    assert_eq!(probe_seconds(&banner.replace("00:01:02.50", "N/A")), None);
    assert_eq!(probe_seconds(&banner.replace("Video", "Audio")), None);
}

fn setup() -> (Environment, tempfile::TempDir) {
    let (env, temp) = Environment::setup();
    std::fs::write(temp.path().join("in.webm"), b"video").unwrap();
    std::fs::write(
        temp.path().join("lines.json"),
        r#"[{"at":0,"text":"First line."},{"at":5,"text":"Second line."}]"#,
    )
    .unwrap();
    mode(
        "NARRATE_VIDEO",
        temp.path().join("in.webm").to_str().unwrap(),
    );
    mode(
        "NARRATE_LINES",
        temp.path().join("lines.json").to_str().unwrap(),
    );
    (env, temp)
}

#[test]
fn narrating_voices_each_line_and_muxes_one_audio_track() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = setup();
    let output = narrate_video(temp.path()).unwrap();
    assert!(std::fs::read(&output).unwrap().starts_with(b"encoded:"));
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert_eq!(log.matches("curl ").count(), 2);
    assert!(log.contains("Authorization: Bearer stub-key"));
    assert!(log.contains("make --no-print-directory ship-tools"));
    assert!(log.contains("-map 0:v:0 -map 1:a:0 -c:v copy -c:a libopus"));
}

#[test]
fn narrating_refuses_a_line_past_the_video_before_any_speech_request() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = setup();
    std::fs::write(
        temp.path().join("lines.json"),
        r#"[{"at":0,"text":"First."},{"at":12,"text":"Late."}]"#,
    )
    .unwrap();
    let error = narrate_video(temp.path()).unwrap_err().to_string();
    assert!(error.contains("line 2 starts at or past"), "{error}");
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert!(!log.contains("curl "));
}

#[test]
fn narrating_reports_speech_failures() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = setup();
    mode("SHOWCASE_CURL_MODE", "fail");
    let error = narrate_video(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider refused request"), "{error}");
}
