use super::{fixture, read};
use crate::perf_comment::render;
use std::fs;
use tempfile::tempdir;

#[test]
fn an_instruction_row_carries_the_gates_note() {
    let dir = tempdir().expect("directory");
    let path = dir.path().join("pr");
    fixture(&path, &[], &[("simulation::advance", 102)], 1000, 100, None);
    let mut ci: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("ci.json")).expect("ci.json")).expect("JSON");
    ci["comparisons"][0]["note"] = "warning +2.0% over baseline 100".into();
    fs::write(path.join("ci.json"), ci.to_string()).expect("ci.json");
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        devs: &[],
        pr: Some(&read(&path)),
        jobs: &[],
        wasm: None,
        noise: &noise,
    });
    assert!(comment.contains(
        "| Instructions: simulation::advance | — | 102 | — | deterministic; warning +2.0% over baseline 100 |"
    ));
}
