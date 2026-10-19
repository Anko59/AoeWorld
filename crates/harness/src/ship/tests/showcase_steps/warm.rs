//! The readiness probe of a take that starts a skirmish: which maps on which
//! servers it warms before filming.
use super::{Origins, Storyboard, plan};
use crate::ship::showcase::skirmish_maps;
use serde_json::{Value, json};

fn maps_of(scenes: Value, app: &str, before: Option<&str>) -> Vec<(u16, Value)> {
    let board = Storyboard::parse(&json!({"title": "t", "scenes": scenes}).to_string()).unwrap();
    let origins = Origins::select(&board, app, before).unwrap();
    let durations = vec![3000; board.scenes.len()];
    skirmish_maps(&plan(&board, &durations, &origins).unwrap())
        .into_iter()
        .collect()
}

fn scene(build: &str, steps: Value) -> Value {
    json!({"kind": "browser", "build": build, "path": "/?skipIntro=1", "caption": "c",
        "seconds": 1, "steps": steps})
}

#[test]
fn each_skirmish_start_names_the_map_the_setup_form_submits() {
    let maps = maps_of(
        json!([scene(
            "after",
            json!([{"start_skirmish": {"preset": "finland", "size": "tiny"}}])
        )]),
        "http://127.0.0.1:8123/",
        None,
    );
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].0, 8123);
    let request = &maps[0].1;
    assert_eq!(request["center_latitude_e7"], 615_000_000);
    assert_eq!(request["center_longitude_e7"], 255_000_000);
    assert_eq!(request["requested_side_meters"], 30_000);
}

#[test]
fn an_unchosen_size_is_the_forms_default_and_repeats_count_once() {
    let start = json!([{"start_skirmish": {"preset": "alps"}}, {"wait_ms": 10}]);
    let maps = maps_of(
        json!([scene("after", start.clone()), scene("after", start)]),
        "http://127.0.0.1:8123/",
        None,
    );
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].1["requested_side_meters"], 480_000);
}

#[test]
fn each_build_is_warmed_on_its_own_port_and_other_scenes_need_nothing() {
    let start = json!([{"start_skirmish": {"preset": "paris", "size": "tiny"}}]);
    let maps = maps_of(
        json!([
            scene("before", start.clone()),
            scene("after", start),
            scene("after", json!([{"wait_ms": 10}])),
        ]),
        "http://127.0.0.1:8123/",
        Some("http://127.0.0.1:8091/"),
    );
    let ports: Vec<u16> = maps.iter().map(|(port, _)| *port).collect();
    assert_eq!(ports, [8091, 8123]);
}
