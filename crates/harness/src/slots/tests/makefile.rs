//! Audits of the Makefile and make/*.mk, so a new recipe cannot forget the heavy slot: every
//! container that a recipe starts forwards `AOE_HEAVY_SLOT` (through `$(HEAVY_ENV)`), and every
//! gate whose recipe runs a container outside a harness command runs under the slot wrapper.
use crate::gates::registry::Registry;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const FORWARDS: &str = "$(HEAVY_ENV)";

pub(super) fn repository() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The Makefile and every make/*.mk, by repository-relative name and text.
pub(super) fn make_files(root: &Path) -> Vec<(String, String)> {
    let mut names = vec!["Makefile".to_owned()];
    for entry in fs::read_dir(root.join("make")).expect("make/") {
        let name = entry.expect("entry").file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".mk") {
            names.push(format!("make/{name}"));
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = fs::read_to_string(root.join(&name)).expect(&name);
            (name, joined_continuations(&text))
        })
        .collect()
}

/// The text with every `\` continuation joined into one line, as make reads it, so a list
/// written one item per line is audited like the single line it means.
pub(super) fn joined_continuations(text: &str) -> String {
    let mut out = String::new();
    let mut continuing = false;
    for line in text.lines() {
        let body = if continuing { line.trim_start() } else { line };
        continuing = body.ends_with('\\');
        out.push_str(body.strip_suffix('\\').map_or(body, str::trim_end));
        out.push(if continuing { ' ' } else { '\n' });
    }
    out
}

#[test]
fn a_list_continued_over_lines_reads_as_one_definition() {
    let text = "SLOT_ENV := \\\n  -e A \\\n  -e B\nother:\n\t@echo hi\n";
    let joined = joined_continuations(text);
    assert_eq!(joined, "SLOT_ENV := -e A -e B\nother:\n\t@echo hi\n");
    assert_eq!(assigned(joined.lines().next().unwrap()), Some("SLOT_ENV"));
}

/// The variable a definition line assigns (`NAME :=`, `NAME =`, `NAME ?=`, `override NAME =`).
fn assigned(line: &str) -> Option<&str> {
    let line = line.strip_prefix("override ").unwrap_or(line);
    let name = line
        .split(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
        .next()?;
    let rest = line[name.len()..].trim_start();
    let assigns = rest.starts_with(":=") || rest.starts_with('=') || rest.starts_with("?=");
    (!name.is_empty() && assigns).then_some(name)
}

/// The variables whose definition has `seed` in it, or refers to one that does (a closure).
fn variables_reaching(texts: &str, seed: &str) -> BTreeSet<String> {
    let definitions: Vec<(&str, &str)> = texts
        .lines()
        .filter_map(|line| Some((assigned(line)?, line)))
        .collect();
    let mut found = BTreeSet::new();
    loop {
        let before = found.len();
        for (name, line) in &definitions {
            let refers = found
                .iter()
                .any(|other| line.contains(&format!("$({other})")));
            if line.contains(seed) || refers {
                found.insert((*name).to_owned());
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

fn refers(line: &str, variables: &BTreeSet<String>) -> bool {
    variables
        .iter()
        .any(|name| line.contains(&format!("$({name})")))
}

fn joined(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Container lines (a literal `docker run`, or a variable that holds one) that do not forward
/// the heavy slot, as `file:line: text`.
fn unforwarded(files: &[(String, String)]) -> Vec<String> {
    let all = joined(files);
    let forwarding = variables_reaching(&all, FORWARDS);
    let containers = variables_reaching(&all, "docker run");
    let mut gaps = Vec::new();
    for (name, text) in files {
        for (number, line) in text.lines().enumerate() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            let container = line.contains("docker run") || refers(line, &containers);
            if container && !line.contains(FORWARDS) && !refers(line, &forwarding) {
                gaps.push(format!("{name}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    gaps
}

/// The recipe lines of `target`: the tab-indented lines after its rule, in the first file that has it.
pub(super) fn recipe<'a>(files: &'a [(String, String)], target: &str) -> Option<Vec<&'a str>> {
    for (_, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        let Some(at) = lines.iter().position(|line| is_rule_for(line, target)) else {
            continue;
        };
        return Some(
            lines[at + 1..]
                .iter()
                .take_while(|line| line.starts_with('\t'))
                .copied()
                .collect(),
        );
    }
    None
}

fn is_rule_for(line: &str, target: &str) -> bool {
    !line.starts_with(['\t', '#', '.'])
        && !line.contains(":=")
        && line
            .split_once(':')
            .is_some_and(|(name, _)| name.trim() == target)
}

#[test]
fn every_container_a_recipe_starts_forwards_the_heavy_slot() {
    let gaps = unforwarded(&make_files(&repository()));
    assert!(gaps.is_empty(), "these do not forward the slot:\n{gaps:#?}");
}

#[test]
fn the_forwarding_audit_catches_a_container_that_drops_the_slot() {
    let files = [(
        "Makefile".to_owned(),
        [
            "HEAVY_ENV := -e AOE_HEAVY_SLOT",
            "DOCKER_RUN := docker run $(HEAVY_ENV) img",
            "BROWSER := docker run img",
            "lint:",
            "\t@$(DOCKER_RUN) cargo test",
            "\t@docker run --rm img cargo test",
            "\t@$(BROWSER) npm test",
            "\t@mkdir -p reports",
        ]
        .join("\n"),
    )];
    let gaps = unforwarded(&files);
    assert_eq!(gaps.len(), 3, "{gaps:#?}");
    assert!(gaps[0].starts_with("Makefile:3:"), "{gaps:#?}");
    assert!(gaps[1].starts_with("Makefile:6:"), "{gaps:#?}");
    assert!(gaps[2].starts_with("Makefile:7:"), "{gaps:#?}");
}

#[test]
fn a_gate_that_runs_containers_outside_a_harness_command_runs_under_the_slot() {
    let root = repository();
    let registry = Registry::load(&root).expect("the committed registry");
    let files = make_files(&root);
    let containers = variables_reaching(&joined(&files), "docker run");
    let mut problems = Vec::new();
    for gate in registry.gates.iter().filter(|gate| !gate.r#static) {
        let Some(target) = gate.command.strip_prefix("make ") else {
            problems.push(format!("{}: its command is not a make target", gate.id));
            continue;
        };
        let Some(lines) = recipe(&files, target) else {
            problems.push(format!("{target}: no make target of that name"));
            continue;
        };
        let bare = lines.iter().any(|line| {
            (line.contains("docker run") || refers(line, &containers))
                && !line.contains("aoe-harness")
                && !line.contains("crates/harness/Cargo.toml")
        });
        let wrapped = lines
            .iter()
            .any(|line| line.contains(&format!("$(SLOT_RUN) {target} -- ")));
        if bare && !wrapped {
            problems.push(format!(
                "{target}: a container runs outside any harness command; run the steps as `$(SLOT_RUN) {target} -- make {target}-steps`"
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Each variable's definition line, by name (the first definition wins).
pub(super) fn definitions(files: &[(String, String)]) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for (_, text) in files {
        for line in text.lines() {
            if let Some(name) = assigned(line) {
                found
                    .entry(name.to_owned())
                    .or_insert_with(|| line.to_owned());
            }
        }
    }
    found
}

/// The upper-case variables a line references as `$(NAME)`.
fn references(line: &str) -> Vec<String> {
    line.split("$(")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            (!name.is_empty() && rest[name.len()..].starts_with(')')).then_some(name)
        })
        .collect()
}

/// What the lines pass into containers (`-e NAME`) and what make reads from the environment
/// (a referenced variable that no makefile defines), through every variable they reach.
fn environment_read(
    lines: &[&str],
    definitions: &BTreeMap<String, String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut forwarded, mut read, mut seen) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut pending: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
    while let Some(text) = pending.pop() {
        for pair in text.split_whitespace().collect::<Vec<_>>().windows(2) {
            if pair[0] == "-e" && !pair[1].contains('=') {
                forwarded.insert(pair[1].to_owned());
            }
        }
        for name in references(&text) {
            match definitions.get(&name) {
                Some(definition) if seen.insert(name.clone()) => pending.push(definition.clone()),
                Some(_) => {}
                None => {
                    read.insert(name);
                }
            }
        }
    }
    (forwarded, read)
}

#[test]
fn the_slot_wrapper_passes_every_environment_variable_its_steps_read() {
    let files = make_files(&repository());
    let definitions = definitions(&files);
    let wrapper = definitions.get("SLOT_RUN").expect("SLOT_RUN is defined");
    let (provided, _) = environment_read(&[wrapper.as_str()], &definitions);
    let mut steps = Vec::new();
    for target in steps_targets(&files) {
        steps.extend(recipe(&files, &target).expect("a steps target has a recipe"));
    }
    assert!(!steps.is_empty(), "the wrapped gates have steps");
    let (forwarded, read) = environment_read(&steps, &definitions);
    let missing: BTreeSet<String> = forwarded
        .union(&read)
        .filter(|name| !provided.contains(*name))
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "SLOT_RUN does not pass these to the steps (add them to SLOT_ENV): {missing:?}"
    );
}

/// The `<target>-steps` targets the slot wrapper runs.
fn steps_targets(files: &[(String, String)]) -> Vec<String> {
    let mut targets: Vec<String> = files
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter(|line| !line.starts_with(['\t', '#', '.']) && !line.contains(":="))
        .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim().to_owned()))
        .filter(|name| name.ends_with("-steps"))
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

#[test]
fn direct_compile_heavy_tests_wrap_the_steps_before_cargo() {
    let files = make_files(&repository());
    for target in ["test-unit", "test-wasm", "test-harness"] {
        let lines = recipe(&files, target).unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains(&format!("$(SLOT_RUN) {target} -- "))),
            "{target}: {lines:?}"
        );
        assert!(recipe(&files, &format!("{target}-steps")).is_some());
    }
}

#[test]
fn bootstrap_compilation_has_a_small_hard_resource_envelope() {
    let files = make_files(&repository());
    let definitions = definitions(&files);
    let wrapper = &definitions["SLOT_BOOTSTRAP"];
    assert!(
        !definitions["SLOT_RUN"].contains(" cargo run "),
        "slot execution must use the built binary"
    );
    assert!(
        recipe(&files, "slot-bootstrap")
            .unwrap()
            .iter()
            .any(|line| line.contains("flock"))
    );
    for target in steps_targets(&files) {
        let gate = target.strip_suffix("-steps").unwrap();
        let rule = files
            .iter()
            .flat_map(|(_, text)| text.lines())
            .find(|line| is_rule_for(line, gate))
            .unwrap();
        if recipe(&files, gate)
            .unwrap()
            .iter()
            .any(|line| line.contains("$(SLOT_RUN)"))
        {
            assert!(
                rule.contains("slot-bootstrap"),
                "{gate} needs the bounded bootstrap"
            );
        }
    }
    for required in ["--memory=2g", "--memory-swap=2g", "--cpus=1", "--jobs=1"] {
        assert!(
            wrapper.contains(required),
            "unbounded pre-slot bootstrap: missing {required}"
        );
    }
}

#[test]
fn validated_slot_admission_reaches_every_container() {
    let files = make_files(&repository());
    for variable in ["AOE_HEAVY_SLOT", "AOE_HEAVY_DIR"] {
        assert!(definitions(&files)["HEAVY_ENV"].contains(&format!("-e {variable}")));
    }
    assert!(unforwarded(&files).is_empty());
}

#[test]
fn the_e2e_variables_the_harness_reads_reach_both_containers() {
    // `make test-e2e` runs the harness two containers deep (the slot wrapper,
    // then test-e2e-steps); a variable set in CI is lost unless both forward it.
    // The harness reads these itself, so the Make-step audit above cannot see them.
    let files = make_files(&repository());
    let slot_env = definitions(&files)["SLOT_ENV"].clone();
    let steps = recipe(&files, "test-e2e-steps")
        .expect("test-e2e-steps recipe")
        .join("\n");
    for variable in [
        "AOE_E2E_SHARD",
        "AOE_E2E_WORKERS",
        "AOE_E2E_SPECS",
        "AOE_E2E_PORT_RANGE",
    ] {
        let flag = format!("-e {variable}");
        assert!(
            slot_env.contains(&flag),
            "SLOT_ENV lacks {flag}: {slot_env}"
        );
        assert!(
            steps.contains(&flag),
            "test-e2e-steps lacks {flag}: {steps}"
        );
    }
}

#[test]
fn the_browser_port_range_reaches_wasm_runner_containers() {
    let files = make_files(&repository());
    for target in ["test-wasm-steps", "test-wasm-focused-steps"] {
        let steps = recipe(&files, target).expect("WASM recipe").join("\n");
        assert!(steps.contains("-e AOE_E2E_PORT_RANGE"), "{target}: {steps}");
    }
}

/// `make help` echoes each line inside single quotes, so an apostrophe in the text ends the
/// string and the shell fails with "Unterminated quoted string". Every echo must close its quote.
#[test]
fn help_lines_close_their_single_quoted_echo() {
    for (name, text) in make_files(&repository()) {
        for (number, line) in text.lines().enumerate() {
            if let Some(echoed) = line.trim_start().strip_prefix("@echo '") {
                let echoed = echoed.trim_end();
                assert!(
                    echoed.ends_with('\'') && !echoed[..echoed.len() - 1].contains('\''),
                    "{name}:{}: an apostrophe inside a single-quoted echo breaks `make help`: {line}",
                    number + 1
                );
            }
        }
    }
}

#[test]
fn dedicated_de_test_mounts_the_install_read_only_and_runs_real_file_tests() {
    let files = make_files(&repository());
    let wrapper = recipe(&files, "test-assets-de").expect("dedicated real-file Make target");
    assert!(
        wrapper
            .iter()
            .any(|line| line.contains("$(SLOT_RUN) test-assets-de -- "))
    );
    let steps = recipe(&files, "test-assets-de-steps")
        .expect("real-file steps")
        .join("\n");
    assert!(
        steps.contains("$(DE_DIR),/de-install,1)"),
        "read-only install mount: {steps}"
    );
    assert!(steps.contains("-e AOE_DE_DIR=/de-install"));
    assert!(steps.contains("-p aoe-assets") && steps.contains("test(real_house)"));
    assert!(
        steps.contains("--no-tests fail"),
        "a missing test must fail"
    );
    assert!(definitions(&files)["SLOT_ENV"].contains("-e DE_DIR"));
}
