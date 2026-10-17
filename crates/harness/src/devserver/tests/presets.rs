use super::Health;
use crate::devserver::presets::{PRESETS, SIZES, ready, request, wanted};
use serde_json::json;
use std::path::PathBuf;

fn web(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/start")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_preset_table_is_the_start_screens() {
    let views = web("views.js");
    for (id, latitude, longitude) in PRESETS {
        let line = views
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("['{id}',")))
            .unwrap_or_else(|| panic!("no preset {id} in views.js"));
        assert!(
            line.contains(&format!("{latitude}, {longitude}]")),
            "{id}: {line} vs {latitude}, {longitude}"
        );
    }
    let table = views
        .split("export const presets = [")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .expect("the presets table");
    assert_eq!(
        table
            .lines()
            .filter(|l| l.trim_start().starts_with("['"))
            .count(),
        PRESETS.len()
    );
    for (_, km) in SIZES {
        assert!(
            views.contains(&format!("<option value=\"{km}\"")),
            "size {km}"
        );
    }
}

#[test]
fn the_request_has_the_fields_the_setup_form_submits() {
    let sequence = web("sequence.js");
    for needle in [
        "schema_version: 1",
        "year_ce: 600, seed: 1, reconstruction_profile: 'circa600_v1'",
        "preparation: 'automatic', hydrology_mode: 'automatic'",
    ] {
        assert!(sequence.contains(needle), "{needle}");
    }
    assert!(web("views.js").contains("id=\"start-scale\" aria-label=\"Map compression\" type=\"number\" min=\"1\" max=\"10000\" step=\"0.001\" value=\"30\""));
    let value = request(615_000_000, 255_000_000, 30_000);
    assert_eq!(
        value["compression"],
        json!({"numerator": 30, "denominator": 1})
    );
}

#[test]
fn finland_tiny_is_always_wanted_and_a_map_is_never_listed_twice() {
    let marks = r#"{"maps": {"finland": {"center_latitude_e7": 615000000, "center_longitude_e7": 255000000, "requested_side_meters": 30000},
        "france": {"center_latitude_e7": 462000000, "center_longitude_e7": 22000000, "requested_side_meters": 1200000}},
        "bookmarks": [{"name": "a", "map": "finland", "x": 1, "y": 1, "zoom": 1}]}"#;
    let list = wanted(Some(marks), None).expect("wanted");
    assert_eq!(
        list.len(),
        2,
        "finland tiny and the finland bookmark map are one request"
    );
    assert_eq!(list[0], request(615_000_000, 255_000_000, 30_000));
    assert_eq!(list[1], request(462_000_000, 22_000_000, 1_200_000));
    let more = wanted(None, Some("alps:tiny, paris:small")).expect("extra");
    assert_eq!(more.len(), 3);
    assert_eq!(more[1], request(466_000_000, 105_000_000, 30_000));
    assert!(wanted(None, Some("alps")).is_err());
    assert!(wanted(None, Some("mars:tiny")).is_err());
    assert!(wanted(None, Some("alps:huge")).is_err());
}

#[test]
fn a_map_is_ready_only_when_its_job_completed() {
    let wanted = vec![
        request(615_000_000, 255_000_000, 30_000),
        request(462_000_000, 22_000_000, 1_200_000),
    ];
    let jobs = r#"[{"id":0,"state":"completed","request":{"center_latitude_e7":615000000,"center_longitude_e7":255000000,"requested_side_meters":30000}},
        {"id":1,"state":"running","request":{"center_latitude_e7":462000000,"center_longitude_e7":22000000,"requested_side_meters":1200000}}]"#;
    let server = Health::serving("x", jobs);
    assert_eq!(ready(server.port, &wanted), (1, 2));
    assert_eq!(
        ready(1, &wanted),
        (0, 2),
        "a server that does not answer has prepared nothing"
    );
}
