//! Audits guard the launch boundaries, including containers outside registry gates.
use std::{
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    option_env!("CARGO_MANIFEST_DIR").map_or_else(
        || std::env::current_dir().unwrap(),
        |dir| Path::new(dir).join("../.."),
    )
}

fn files() -> Vec<(PathBuf, String)> {
    let root = root();
    let mut paths = vec![root.join("Makefile")];
    paths.extend(
        fs::read_dir(root.join("make"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "mk")),
    );
    paths
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect()
}

#[test]
fn every_make_workload_container_has_shared_caps_and_immediate_admission() {
    for (path, text) in files() {
        let text = text.replace("\\\n", " ");
        for line in text
            .lines()
            .filter(|line| line.contains("docker run") && !line.trim_start().starts_with('#'))
        {
            if path.ends_with("resources.mk") {
                // The compiler of the std-only launcher cannot invoke that launcher yet.
                for flag in ["--memory=256m", "--memory-swap=256m", "--cpu-shares="] {
                    assert!(line.contains(flag), "{}: {line}", path.display());
                }
            } else {
                assert!(
                    line.contains("$(RESOURCE_RUN)"),
                    "no admission: {}: {line}",
                    path.display()
                );
                assert!(
                    (line.contains("$(RESOURCE_CAP)") || line.contains("$(DOCKER_RUN)")),
                    "no caps: {}: {line}",
                    path.display()
                );
            }
            assert!(
                !line.contains("--cpus"),
                "CPU quota: {}: {line}",
                path.display()
            );
        }
    }
    assert!(
        fs::read_to_string(root().join("Makefile"))
            .unwrap()
            .contains("RESOURCE_CAP := $(file <gates/container.args)")
    );
}

#[test]
fn long_make_targets_acquire_before_workload_prerequisites_and_recipes() {
    let long = [
        "perf-census-instructions",
        "coverage",
        "test-e2e",
        "perf-smoke",
        "perf-ci",
        "perf-full",
        "perf-instructions",
        "perf-groups",
        "perf-pressure",
        "perf-scale",
        "perf-scale-smoke",
        "perf-soak-10",
        "perf-soak-30",
        "perf-stress",
        "perf-late-game",
        "scale-match",
        "preflight",
        "preflight-unit",
        "perf-baseline-propose",
        "release-build",
        "perf-groups-instructions",
        "perf-groups-profile",
        "perf-late-game-instructions",
        "scale-match-smoke",
    ];
    let files = files();
    let resources = fs::read_to_string(root().join("make/resources.mk")).unwrap();
    let guarded: Vec<_> = resources
        .lines()
        .find_map(|line| line.strip_prefix("LONG_GOALS := "))
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(guarded.len(), long.len());
    for target in long {
        assert!(guarded.contains(&target), "missing pre-check for {target}");
        let prefix = format!("{target}:");
        let (_, text) = files
            .iter()
            .find(|(_, text)| text.lines().any(|line| line.starts_with(&prefix)))
            .unwrap();
        let mut lines = text.lines().skip_while(|line| !line.starts_with(&prefix));
        let declaration = lines.next().unwrap();
        assert_eq!(
            declaration,
            format!("{target}: resource-bootstrap"),
            "{target}: prerequisites must run inside admission"
        );
        let recipe = lines.next().unwrap();
        assert!(
            recipe.contains(&format!("$(RESOURCE_LONG) {target} -- ")),
            "{target}: {recipe}"
        );
    }
}

#[test]
fn resource_bootstrap_builds_the_tool_image_before_using_it() {
    let output = std::process::Command::new("make")
        .current_dir(root())
        .args(["-n", "-B", ".cache/resource-runner"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let plan = String::from_utf8(output.stdout).unwrap();
    let build = plan
        .find("-f docker/rust-tools.Dockerfile")
        .expect("tool image build");
    let compile = plan.find("rustc --edition=2024").expect("runner compile");
    assert!(build < compile, "{plan}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Circular"));
}

#[test]
fn browser_wrapper_observes_cgroup_events_before_container_removal() {
    let text = fs::read_to_string(root().join("Makefile")).unwrap();
    let wrapper = text
        .lines()
        .find(|line| line.starts_with("BROWSER_RUN :="))
        .unwrap();
    assert!(
        wrapper.ends_with("$(BROWSER_IMAGE) $(RESOURCE_OBSERVER)"),
        "{wrapper}"
    );
}
