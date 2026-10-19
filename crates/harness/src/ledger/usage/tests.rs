use super::{claude, codex, iso, openrouter};
use std::collections::{BTreeMap, HashSet};

#[test]
fn timestamps_render_in_utc() {
    assert_eq!(iso(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso(1_792_878_384), "2026-10-24T21:46:24Z");
    assert_eq!(iso(951_782_400 + 86_399), "2000-02-29T23:59:59Z");
}

fn lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().map(str::to_owned)
}

#[test]
fn claude_tokens_are_summed_per_model_once_per_message_in_the_window() {
    let log = r#"{"timestamp":"2026-10-17T10:00:00Z","message":{"id":"a","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000}}}
{"timestamp":"2026-10-17T10:00:01Z","message":{"id":"a","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000}}}
{"timestamp":"2026-10-17T11:00:00Z","message":{"id":"b","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":2}}}
{"timestamp":"2026-10-01T11:00:00Z","message":{"id":"old","model":"claude-opus-5-5","usage":{"input_tokens":999}}}
{"timestamp":"2026-10-17T12:00:00Z","message":{"id":"s","model":"<synthetic>","usage":{"input_tokens":7}}}
{"timestamp":"2026-10-17T12:00:00Z","message":{"id":"h","model":"claude-haiku-5-5","usage":{"input_tokens":3}}}
not json "usage"
"#;
    let (mut totals, mut seen) = (BTreeMap::new(), HashSet::new());
    claude::tally(lines(log), "2026-10-11T00:00:00Z", &mut totals, &mut seen);
    let opus = &totals["claude-opus-5-5"];
    assert_eq!(
        (
            opus.messages,
            opus.input,
            opus.output,
            opus.cache_write,
            opus.cache_read
        ),
        (2, 11, 7, 100, 1000)
    );
    assert_eq!(totals["claude-haiku-5-5"].input, 3);
    assert_eq!(totals.len(), 2);
    let text = claude::render(&totals, 4);
    assert!(text.contains("remaining: unknown: "), "{text}");
}

#[test]
fn a_missing_claude_directory_is_unknown() {
    let text = claude::section(std::path::Path::new("/nonexistent/claude"), 0);
    assert!(
        text.contains("unknown: /nonexistent/claude does not exist"),
        "{text}"
    );
}

const SNAPSHOT: &str = r#"{"timestamp":"2026-10-18T00:44:22.662Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"plan_type":"prolite","primary":{"used_percent":1.0,"window_minutes":10080,"resets_at":1792878384},"secondary":null}}}"#;

#[test]
fn codex_reports_the_newest_snapshot_with_a_window() {
    let older = SNAPSHOT
        .replace("00:44:22", "00:40:00")
        .replace("1.0,", "0.5,");
    let empty = r#"{"timestamp":"2026-10-18T01:00:00.000Z","payload":{"rate_limits":{"primary":null,"secondary":null}}}"#;
    let snapshot = codex::latest(lines(&format!("{SNAPSHOT}\n{older}\n{empty}\nnoise\n"))).unwrap();
    assert_eq!(snapshot.windows[0].used_percent, 1.0);
    let text = codex::render(Some(snapshot), 1_792_800_000);
    assert!(
        text.contains("10080 min window: 1.0% used, 99.0% left, resets 2026-10-24T21:46:24Z"),
        "{text}"
    );
}

#[test]
fn codex_without_a_live_window_is_unknown() {
    let snapshot = codex::latest(lines(SNAPSHOT));
    let expired = codex::render(snapshot, 1_800_000_000);
    assert!(expired.contains("unknown: that window reset"), "{expired}");
    assert!(codex::render(None, 0).contains("unknown: no session"));
    assert!(codex::latest(lines("{}\n")).is_none());
}

#[test]
fn openrouter_credits_and_refusals() {
    let ok = r#"{"data":{"total_credits":522,"total_usage":517.5}}"#;
    assert_eq!(openrouter::parse_credits(ok), Ok((522.0, 517.5)));
    assert!(openrouter::render(openrouter::parse_credits(ok)).contains("credits left: $4.50"));
    let refused = openrouter::parse_credits(r#"{"error":{"message":"Key limit exceeded"}}"#);
    assert!(
        openrouter::render(refused).contains("unknown: OpenRouter refused: Key limit exceeded")
    );
    assert!(openrouter::parse_credits("<html>").is_err());
    assert!(openrouter::parse_credits(r#"{"data":{}}"#).is_err());
    assert!(
        openrouter::render(Err("no OPENROUTER_API_KEY".into()))
            .contains("unknown: no OPENROUTER_API_KEY")
    );
}
