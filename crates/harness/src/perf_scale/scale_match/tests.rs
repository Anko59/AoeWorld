use super::{
    MapSource, Options, browser, http, procs,
    report::{self, Sample, TickRow},
    run,
};
use clap::Parser;
use serde_json::json;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    options: Options,
}

fn options(arguments: &[&str]) -> Options {
    Cli::parse_from(std::iter::once("scale-match").chain(arguments.iter().copied())).options
}

fn row(tick: u64, total_us: u64, sim_us: u64, units: u64, missed: bool) -> TickRow {
    TickRow {
        tick,
        total_us,
        sim_us,
        late_us: 0,
        orders: 10,
        units,
        missed,
        phases_us: [0; 3],
    }
}

#[test]
fn defaults_describe_the_promised_scale() {
    let defaults = options(&[]);
    assert_eq!(defaults.players, 100);
    assert_eq!(defaults.humans, 1);
    assert_eq!(defaults.tiles, 16_000);
    assert_eq!(defaults.population_cap, 1_000);
    assert_eq!(defaults.map, MapSource::Procedural);
    assert!((defaults.minutes - 20.0).abs() < f64::EPSILON);
}

#[test]
fn the_match_body_names_slots_rules_and_a_france_map() {
    let body = run::match_body(&options(&["--players", "4", "--start-villagers", "30"])).unwrap();
    assert_eq!(body["slots"], json!(["human", "ai", "ai", "ai"]));
    assert_eq!(body["population_cap"], 1_000);
    assert_eq!(body["start_units"], json!({"villagers": 30, "militia": 0}));
    let map = &body["map"];
    // 16,000 two-metre tiles at 30:1 is 960 km of France.
    assert_eq!(map["requested_side_meters"], 960_000);
    assert_eq!(map["compression"]["numerator"], 30);
    assert_eq!(map["center_latitude_e7"], 466_000_000);
    let flat = run::match_body(&options(&["--map", "flat", "--tiles", "500"])).unwrap();
    assert_eq!(flat["flat"]["tiles_per_side"], 500);
    assert!(flat.get("start_units").is_none());
    assert!(run::match_body(&options(&["--map", "package"])).is_err());
}

#[test]
fn http_responses_parse_plain_and_chunked() {
    let plain = http::parse(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").unwrap();
    assert_eq!((plain.status, plain.body.as_slice()), (200, &b"{}"[..]));
    let chunked = http::parse(
        b"HTTP/1.1 400 Bad Request\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=1\r\nde\r\n0\r\n\r\n",
    )
    .unwrap();
    assert_eq!(
        (chunked.status, chunked.body.as_slice()),
        (400, &b"abcde"[..])
    );
    assert!(http::parse(b"HTTP/1.1 200 OK\r\n").is_err());
}

#[test]
fn docker_stats_lines_become_cores_and_mebibytes() {
    let sample = procs::parse_stats_line("187.50% 1.5GiB / 14GiB").unwrap();
    assert!((sample.cores - 1.875).abs() < 1e-9);
    assert!((sample.rss_mib - 1536.0).abs() < 1e-9);
    assert!(procs::parse_stats_line("garbage").is_none());
    assert!(procs::process(std::process::id()).is_some());
}

#[test]
fn real_match_recording_is_an_explicit_browser_option() {
    let options = Cli::try_parse_from(["scale-match", "--browser", "--record-video"])
        .unwrap()
        .options;
    let args = browser::docker_args(
        &options,
        std::path::Path::new("/repo"),
        std::path::Path::new("/repo/reports/combat.json"),
    );
    assert!(args.contains(&"SCALE_RECORD_VIDEO=1".to_owned()));
}

#[test]
fn live_match_report_retains_combat_orders_and_hierarchy_phase_timings() {
    let rows = [1_000, 3_000].map(|combat| {
        TickRow::from_json(&json!([
            7, 30_000, 20_000, 0, 10, 1_000, 0, 0,
            {"orders": 2_000, "combat": combat, "hierarchy": 1_500}
        ]))
        .unwrap()
    });
    let window = report::window(&rows, 0.1);
    assert_eq!(window["phases_ms"]["orders"]["p95_ms"], 2.0);
    assert_eq!(window["phases_ms"]["combat"]["p95_ms"], 3.0);
    assert_eq!(window["phases_ms"]["hierarchy"]["p95_ms"], 1.5);
}

#[test]
fn tick_rows_parse_and_summarise() {
    let parsed = TickRow::from_json(&json!([
        7, 30_000, 20_000, 0, 10, 1_000, 0, 0,
        {"orders": 0, "combat": 0, "hierarchy": 0}
    ]))
    .unwrap();
    assert_eq!(parsed, row(7, 30_000, 20_000, 1_000, false));
    assert!(TickRow::from_json(&json!([1, 2])).is_none());
    let rows: Vec<_> = (0..100)
        .map(|index| {
            row(
                index,
                40_000 + index * 200,
                30_000,
                1_000 + index,
                index % 10 == 0,
            )
        })
        .collect();
    let window = report::window(&rows, 5.0);
    assert_eq!(window["ticks"], 100);
    assert_eq!(window["missed_ticks"], 10);
    assert_eq!(window["over_50ms"], 49);
    assert_eq!(window["ticks_per_second"], 20.0);
    assert_eq!(window["realtime_factor"], 1.0);
    assert_eq!(window["units_first"], 1_000);
    assert_eq!(window["units_max"], 1_099);
    assert_eq!(window["advance_ms"]["p50_ms"], 30.0);
    // 100 ticks over 150 s of wall time: three windows of the arrival order.
    assert_eq!(report::per_minute(&rows, 150.0).len(), 3);
    assert!(report::per_minute(&[], 60.0).is_empty());
}

#[test]
fn bot_reports_add_up() {
    let totals = report::bot_totals(&[
        json!([{"decisions": 4, "villagers": 10, "accepted": 3, "closed": false, "age": "FEUDAL",
                "stats": {"build_orders": 7, "buildings_completed": 5,
                          "rejected:build:REJECTED_WORLD_LIMIT": 2}}]),
        json!([{"decisions": 6, "villagers": 5, "rejected": 1, "closed": true, "errors": 2,
                "drops": 2, "rejoins": 1, "reconnect_seconds": 1.5}]),
    ]);
    assert_eq!(totals["drops"], 2);
    assert_eq!(totals["feudal"], 1);
    assert_eq!(totals["build_orders"], 7);
    assert_eq!(totals["buildings_completed"], 5);
    assert_eq!(totals["build_rejections"]["REJECTED_WORLD_LIMIT"], 2);
    assert_eq!(totals["rejoins"], 1);
    assert_eq!(totals["sessions_dropped"], 1);
    assert_eq!(totals["reconnect_seconds"], 1.5);
    assert_eq!(totals["bots"], 2);
    assert_eq!(totals["decisions"], 10);
    assert_eq!(totals["villagers"], 15);
    assert_eq!(totals["closed"], 1);
    assert_eq!(totals["errors"], 2);
}

fn finished_report() -> serde_json::Value {
    let rows = vec![
        row(1, 40_000, 30_000, 1_000, false),
        row(2, 60_000, 50_000, 1_100, true),
    ];
    let mut run = report::window(&rows, 0.1);
    run["wall_seconds"] = json!(60.0);
    json!({
        "label": "smoke", "revision": "abc", "dirty": false,
        "config": {"players": 4, "humans": 1, "tiles": 500, "population_cap": 300, "hosts": 1},
        "setup": {"map_kind": "flat", "seeded_units": 60, "partial_starting_kits": 0, "match_create_seconds": 1.5, "bots_connect_seconds": 5.0},
        "run": run,
        "machine": {},
        "bots": report::bot_totals(&[]),
        "browser": {"phases": [{"name": "village", "fps_mean": 31.5, "frame_ms_p50": 30.0, "frame_ms_p95": 41.0, "frames": 900}]},
        "per_minute": report::per_minute(&rows, 60.0),
    })
}

#[test]
fn the_summary_block_replaces_only_what_follows_the_marker() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("match.md");
    std::fs::write(
        &path,
        "# Hand written\n\ntext\n<!-- scale-match:latest -->\nold block",
    )
    .unwrap();
    run::write_summary(&path, &finished_report()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# Hand written\n\ntext\n<!-- scale-match:latest -->"));
    assert!(!text.contains("old block"));
    assert!(text.contains("Latest run: smoke"));
    assert!(text.contains("village 31.5 fps mean"));
    // Writing again does not stack blocks.
    run::write_summary(&path, &finished_report()).unwrap();
    let again = std::fs::read_to_string(&path).unwrap();
    assert_eq!(again.matches("Latest run").count(), 1);
}

#[test]
fn the_timeline_keeps_every_sample() {
    let sample = Sample {
        seconds: 5.0,
        tick: 100,
        units: 2_000,
        server_cpu_cores: 1.5,
        server_rss_mib: 900,
        bots_cpu_cores: 3.0,
        bots_rss_mib: 4_000.0,
    };
    let line = report::timeline(&[sample, sample]);
    assert_eq!(line.len(), 2);
    assert_eq!(line[0]["server_rss_mib"], 900);
}

#[test]
fn the_browser_container_gets_the_pinned_image_the_server_url_and_the_script() {
    let options = options(&["--browser", "--browser-minutes", "3"]);
    let args = browser::docker_args(
        &options,
        std::path::Path::new("/repo"),
        std::path::Path::new("/repo/out.json"),
    );
    assert!(args.contains(&"SCALE_BASE_URL=http://127.0.0.1:9100".to_owned()));
    assert!(args.contains(&"SCALE_SECONDS=180".to_owned()));
    assert!(args.contains(&"aoeworld/browser-tools:1.64.0".to_owned()));
    assert_eq!(args.last().map(String::as_str), Some(browser::SCRIPT));
    assert!(args.windows(2).any(|pair| pair == ["-w", "/repo/browser"]));
}
