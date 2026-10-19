//! The storyboard step library: schema validation, each step's expansion into
//! ops, bookmark resolution and the named timeout (steps.mjs run by node).
use super::super::showcase::{Origins, Storyboard, plan};
use serde_json::{Value, json};

mod warm;

fn board(steps: Value, path: &str) -> String {
    json!({"title": "t", "scenes": [{
        "kind": "browser", "path": path, "caption": "c", "seconds": 1, "steps": steps}]})
    .to_string()
}

fn parse(steps: Value) -> Result<Storyboard, String> {
    Storyboard::parse(&board(steps, "/?skipIntro=1"))
}

fn scripts(steps: Value) -> Value {
    let parsed = parse(steps).unwrap();
    let origins = Origins::select(&parsed, "http://127.0.0.1:8080/", None).unwrap();
    let planned = plan(&parsed, &[3000], &origins).unwrap();
    serde_json::to_value(planned).unwrap()["scenes"][0]["scripts"].clone()
}

fn ops(script: &Value) -> Vec<(String, String)> {
    script["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| {
            let target = op["selector"]
                .as_str()
                .or(op["value"].as_str())
                .unwrap_or("");
            (op["op"].as_str().unwrap().into(), target.into())
        })
        .collect()
}

fn refused(steps: Value, why: &str) {
    let error = parse(steps).unwrap_err();
    assert!(error.contains(why), "{error}");
}

#[test]
fn every_library_step_parses_and_primitive_steps_still_do() {
    parse(json!([
        {"start_skirmish": {"preset": "alps", "colour": "purple", "size": "tiny", "timeout_ms": 90000}},
        {"select_entity": {"kind": "villager", "nth": 1}},
        {"command": {"action": "move", "dx": 200, "dy": -50}},
        {"command": {"action": "gather", "resource": "wood"}},
        {"camera": {"x": 252, "y": 252.5, "zoom": 1.5}},
        {"wait_for": {"selector": "#minimap"}},
        {"wait_for": {"state": "settled"}},
        {"open_panel": {"name": "grid"}},
        {"select": {"selector": "#map-region", "value": "alps"}},
        {"wait_ms": 500}
    ]))
    .unwrap();
}

#[test]
fn bad_library_steps_are_refused_naming_the_step_and_the_field() {
    refused(json!([{"start_skirmish": {"preset": "mars"}}]), "mars");
    refused(
        json!([{"start_skirmish": {"preset": "alps", "seed": 3}}]),
        "seed",
    );
    refused(
        json!([{"start_skirmish": {"preset": "alps", "timeout_ms": 0}}]),
        "above 0",
    );
    refused(
        json!([{"start_skirmish": {"preset": "alps", "timeout_ms": 900000}}]),
        "limited",
    );
    refused(
        json!([{"wait_ms": 1}, {"camera": {"x": 1}}]),
        "step 2: camera takes",
    );
    refused(json!([{"camera": {"x": 1, "y": 1, "zoom": 9}}]), "zoom");
    refused(
        json!([{"camera": {"bookmark": "a", "x": 1, "y": 1, "zoom": 1}}]),
        "camera takes",
    );
    refused(json!([{"wait_for": {}}]), "wait_for takes");
    refused(
        json!([{"wait_for": {"selector": "#a", "state": "game"}}]),
        "not both",
    );
    refused(json!([{"open_panel": {"name": "settings"}}]), "settings");
    // The map creator panel is gone (#451): the error lists what exists.
    refused(
        json!([{"open_panel": {"name": "creator"}}]),
        "expected `overview` or `grid`",
    );
}

#[test]
fn what_the_client_has_no_hook_for_is_refused_before_any_build() {
    refused(
        json!([{"command": {"action": "build"}}]),
        "no build command",
    );
    refused(
        json!([{"command": {"action": "move", "resource": "wood"}}]),
        "not a resource",
    );
    refused(
        json!([{"command": {"action": "gather", "dx": 3}}]),
        "takes a resource",
    );
    refused(
        json!([{"command": {"action": "move", "dx": 9000}}]),
        "limited to",
    );
    refused(
        json!([{"select_entity": {"kind": "villager", "nth": 64}}]),
        "nth is below",
    );
    refused(json!([{"select_entity": {"kind": "wall"}}]), "wall");
}

#[test]
fn a_start_needs_the_menu_page() {
    let steps = json!([{"start_skirmish": {"preset": "paris"}}]);
    let error = Storyboard::parse(&board(steps.clone(), "/")).unwrap_err();
    assert!(error.contains("start_skirmish needs the menu"), "{error}");
    Storyboard::parse(&board(steps, "/?skipIntro=1&renderer=webgl2")).unwrap();
}

#[test]
fn start_skirmish_walks_the_setup_and_waits_for_a_running_match() {
    let value = scripts(
        json!([{"start_skirmish": {"preset": "alps", "colour": "orange", "size": "tiny"}}]),
    );
    let script = &value[0];
    assert!(
        script["label"]
            .as_str()
            .unwrap()
            .starts_with("start_skirmish ")
    );
    assert!(script["label"].as_str().unwrap().contains("alps"));
    let ops = ops(script);
    let at = |op: &str, target: &str| {
        ops.iter()
            .position(|(o, t)| o == op && t.contains(target))
            .unwrap_or_else(|| panic!("no {op} {target} in {ops:?}"))
    };
    let order = [
        at("click", "#single-player"),
        at("check", "Orange"),
        at("click", "[data-preset=\"alps\"]"),
        at("select_option", "#start-size"),
        at("click", "#start-game"),
        at("wait", "data-start-state=\"game\""),
        at("wait", "data-chunks-pending=\"0\""),
        at("wait", "data-raster=\"ready\""),
        at("settle", ""),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    let size = &script["ops"][at("select_option", "#start-size")];
    assert_eq!(size["value"], "30");
    assert!(
        script["ops"]
            .as_array()
            .unwrap()
            .iter()
            .all(|op| op["timeout_ms"] == 180_000)
    );
}

#[test]
fn select_command_camera_and_panels_expand_to_waited_ops() {
    let value = scripts(json!([
        {"select_entity": {"kind": "villager", "nth": 2}},
        {"command": {"action": "move"}},
        {"camera": {"x": 10.5, "y": 20, "zoom": 1.5}},
        {"open_panel": {"name": "overview"}},
        {"wait_for": {"state": "selected", "timeout_ms": 5000}},
        {"wait_ms": 100},
        {"command": {"action": "gather", "resource": "gold"}},
        {"select_entity": {"kind": "building"}}
    ]));
    let select = ops(&value[0]);
    assert_eq!(select[0], ("key".into(), "h".into()));
    let click = &value[0]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["op"] == "click_actor")
        .unwrap()
        .clone();
    assert_eq!(
        (click["actor"].clone(), click["nth"].clone()),
        (json!("Villager"), json!(2))
    );
    assert!(select.last().unwrap().1.contains("#selection"));
    let building = &value[7]["ops"];
    assert!(
        building
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["actor"] == "TownCenter")
    );
    let order = &value[1]["ops"][0];
    assert_eq!(
        (
            order["dx"].clone(),
            order["dy"].clone(),
            order["right"].clone()
        ),
        (json!(128), json!(64), json!(true))
    );
    let camera = ops(&value[2]);
    assert_eq!(camera[0], ("hash".into(), "x=10.5&y=20&zoom=1.5".into()));
    assert_eq!(value[2]["ops"][1]["camera"], json!([10.5, 20.0, 1.5]));
    assert_eq!(camera.last().unwrap().0, "settle");
    let panel = &value[3]["ops"][0];
    assert_eq!(panel["selector"], "#overview-toggle");
    assert_eq!(panel["unless"], "#overview-toggle[aria-expanded=\"true\"]");
    assert_eq!(value[4]["ops"][0]["timeout_ms"], 5000);
    assert_eq!(value[5], Value::Null);
    let gather = ops(&value[6]);
    assert_eq!(value[6]["ops"][0]["kind"], 2);
    assert_eq!(gather[0].0, "gather");
    assert!(gather[1].1.contains("#workers-gold"), "{gather:?}");
}

#[test]
fn a_scene_without_library_steps_plans_no_scripts() {
    let parsed = parse(json!([{"click": "text=Start"}])).unwrap();
    let origins = Origins::select(&parsed, "http://127.0.0.1:8080/", None).unwrap();
    let json = serde_json::to_value(plan(&parsed, &[3000], &origins).unwrap()).unwrap();
    assert!(json["scenes"][0].get("scripts").is_none());
}

#[test]
fn camera_bookmarks_resolve_to_coordinates_from_the_bookmarks_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut parsed = parse(json!([{"camera": {"bookmark": "finland-start"}}])).unwrap();
    super::super::showcase::resolve_bookmarks(&mut parsed, &root).unwrap();
    let value = scripts_of(&parsed);
    assert_eq!(value[0]["ops"][0]["value"], "x=252&y=252&zoom=1");
    let mut unknown = parse(json!([{"camera": {"bookmark": "nowhere"}}])).unwrap();
    let error = super::super::showcase::resolve_bookmarks(&mut unknown, &root).unwrap_err();
    assert!(
        error.contains("scene 1 step 1") && error.contains("nowhere"),
        "{error}"
    );
    let origins = Origins::select(&unknown, "http://127.0.0.1:8080/", None).unwrap();
    let error = plan(&unknown, &[3000], &origins).unwrap_err();
    assert!(error.contains("not resolved"), "{error}");
}

fn scripts_of(parsed: &Storyboard) -> Value {
    let origins = Origins::select(parsed, "http://127.0.0.1:8080/", None).unwrap();
    serde_json::to_value(plan(parsed, &[3000], &origins).unwrap()).unwrap()["scenes"][0]["scripts"]
        .clone()
}

/// Runs `runScript` under node (the harness test image has none, so these
/// checks run wherever node is installed) against a scripted page, and
/// returns what the thrown error said, or `None` without node.
fn run_step(step: Value, page: &str, number: u32) -> Option<String> {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipped: node is not installed in this image");
        return None;
    }
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("steps.mjs", include_str!("../../showcase/steps/steps.mjs")),
        (
            "pagestate.mjs",
            include_str!("../../showcase/steps/pagestate.mjs"),
        ),
        (
            "framing.mjs",
            include_str!("../../showcase/steps/framing.mjs"),
        ),
        (
            "readiness.mjs",
            include_str!("../../compare/capture/readiness.mjs"),
        ),
    ] {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    let value = scripts(json!([step]));
    let runner = dir.path().join("run.mjs");
    std::fs::write(
        &runner,
        format!(
            r#"import {{ runScript }} from "./steps.mjs";
const page = {page};
try {{ await runScript(page, {script}, {number}); console.log("no error"); }} catch (e) {{ console.log(e.message); }}"#,
            script = value[0]
        ),
    )
    .unwrap();
    let out = std::process::Command::new("node")
        .arg(&runner)
        .output()
        .unwrap();
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A page whose waits time out and whose DOM reads as `state`.
fn stuck_page(state: &str) -> String {
    format!(
        r#"{{ on() {{}}, evaluate: async () => ({state}),
  waitForSelector: async () => {{ const e = new Error("locator timed out\nlong log"); e.name = "TimeoutError"; throw e; }} }}"#
    )
}

#[test]
fn a_timeout_names_the_step_and_what_it_waited_for() {
    let page = stuck_page("({})");
    let step = json!({"wait_for": {"selector": "#never", "timeout_ms": 50}});
    let Some(message) = run_step(step, &page, 4) else {
        return;
    };
    assert!(message.contains("showcase step 4 `wait_for"), "{message}");
    assert!(message.contains("#never"), "{message}");
    assert!(message.contains("timed out after"), "{message}");
    assert!(message.contains("limit 50 ms"), "{message}");
}

#[test]
fn a_timeout_says_which_state_the_page_was_stuck_in() {
    let page = stuck_page(
        r#"{ start_state: "loading", connection: "connected", loading_title: "Opening your world", loading_phase: "Opening terrain", raster: "pending", chunks_pending: "3" }"#,
    );
    let step = json!({"wait_for": {"state": "game", "timeout_ms": 50}});
    let Some(message) = run_step(step, &page, 2) else {
        return;
    };
    assert!(message.contains("start_state=\"loading\""), "{message}");
    assert!(
        message.contains("loading_title=\"Opening your world\""),
        "{message}"
    );
    assert!(message.contains("raster=\"pending\""), "{message}");
}

#[test]
fn a_page_that_failed_for_good_fails_the_step_without_waiting_for_the_limit() {
    // The wait never returns; only the page's own failure can end it.
    let page = r#"{ on() {}, evaluate: async () => ({ unsupported: "Unable to start AoeWorld: AoE II assets are unavailable.", start_state: "menu" }),
  waitForSelector: () => new Promise(() => {}) }"#;
    let step = json!({"wait_for": {"state": "game", "timeout_ms": 60000}});
    let started = std::time::Instant::now();
    let Some(message) = run_step(step, page, 1) else {
        return;
    };
    assert!(started.elapsed().as_secs() < 20, "{message}");
    assert!(message.contains("the page failed after"), "{message}");
    assert!(message.contains("assets are unavailable"), "{message}");
    assert!(!message.contains("timed out"), "{message}");
}

#[test]
fn every_state_and_panel_the_docs_name_is_accepted() {
    let docs = include_str!("../../../../../../docs/showcase.md");
    let line = |key: &str| {
        let start = docs.find(key).unwrap_or_else(|| panic!("docs lack {key}"));
        docs[start + key.len()..]
            .split('"')
            .next()
            .unwrap()
            .to_owned()
    };
    for state in line("{\"state\": \"").split('|') {
        parse(json!([{"wait_for": {"state": state}}])).unwrap_or_else(|e| panic!("{state}: {e}"));
    }
    for name in line("{\"name\": \"").split('|') {
        parse(json!([{"open_panel": {"name": name}}])).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn every_module_the_recorder_imports_is_shipped_to_the_work_directory() {
    let work = tempfile::tempdir().unwrap();
    crate::ship::showcase::write_recorder(work.path()).unwrap();
    let mut checked = 0;
    for entry in std::fs::read_dir(work.path()).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for import in text.split("from \"./").skip(1) {
            let name = import.split('"').next().unwrap();
            assert!(
                work.path().join(name).is_file(),
                "{} imports ./{name}, which write_recorder does not ship",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked >= 6, "only {checked} imports found");
}
