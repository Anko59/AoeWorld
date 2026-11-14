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
    // Every rule whose recipe takes a long slot is the list; the Make pre-check
    // must name exactly those goals, so neither can drift from the other. The
    // owning rule counts, not the launcher's argument, which a copy could keep.
    let files = files();
    let mut long = Vec::new();
    for (_, text) in &files {
        let mut rule = "";
        for line in text.lines() {
            if let Some(recipe) = line.strip_prefix('\t') {
                if recipe.contains("$(RESOURCE_LONG)") {
                    long.extend(rule.split_whitespace());
                }
            } else if let Some((targets, _)) = line.split_once(':')
                && !line.contains('=')
                && !line.starts_with('#')
            {
                rule = targets;
            }
        }
    }
    long.sort_unstable();
    long.dedup();
    let resources = fs::read_to_string(root().join("make/resources.mk")).unwrap();
    let mut guarded: Vec<_> = resources
        .lines()
        .find_map(|line| line.strip_prefix("LONG_GOALS := "))
        .unwrap()
        .split_whitespace()
        .collect();
    guarded.sort_unstable();
    assert_eq!(
        guarded, long,
        "LONG_GOALS must name every $(RESOURCE_LONG) recipe"
    );
    for target in long {
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

#[test]
fn only_save_replay_counter_targets_request_perfmon() {
    let text = fs::read_to_string(root().join("make/profiling.mk")).unwrap();
    for target in ["save-replay-steps:", "save-replay-counter-check:"] {
        let recipe = text
            .lines()
            .skip_while(|line| *line != target)
            .nth(1)
            .unwrap();
        assert!(recipe.contains("--cap-add PERFMON"), "{target}: {recipe}");
        assert!(!recipe.contains("seccomp=unconfined"));
    }
    let makefile = fs::read_to_string(root().join("Makefile")).unwrap();
    assert!(!makefile.contains("--cap-add PERFMON"));
}

#[test]
fn save_replay_isolates_cargo_artifacts_from_shared_worktree_targets() {
    let text = fs::read_to_string(root().join("make/profiling.mk")).unwrap();
    let recipe = text
        .lines()
        .skip_while(|line| *line != "save-replay-steps:")
        .nth(1)
        .unwrap();
    assert!(
        recipe.contains("CARGO_TARGET_DIR=$(ROOT)/.cache/save-replay/target"),
        "{recipe}"
    );
}
