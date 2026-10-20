//! PR histories with injected Playwright JSON, through the real red/green classifier.
use super::{commit, repo};
use crate::ship::{duel::focus, git, test_first::red};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub(super) const SPEC: &str = "browser/tests/new.spec.ts";
pub(super) const MODULE: &str = "web/new.mjs";
pub(super) const SOURCE: &str =
    "import { fresh } from '../../web/new.mjs';\ntest('works', () => { fresh(); });\n";
const NAME: &str = "new.spec.ts::works [webgpu]";

pub(super) fn tip(root: &Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

pub(super) fn fixture(snapshot: &Path, message: String, file: &str, suite: bool) -> Value {
    let error = json!({"message": message, "location": {
        "file": snapshot.join(file), "line": 1, "column": 1
    }});
    if suite {
        json!({"errors": [], "suites": [{"file": "tests/new.spec.ts", "errors": [error]}]})
    } else {
        json!({"suites": [], "errors": [error, {"message": "Error: No tests found."}]})
    }
}

pub(super) fn parse(snapshot: &Path, value: Value) -> git::Result<focus::CheckReport> {
    let report = snapshot.join("report.json");
    fs::write(&report, serde_json::to_vec(&value).unwrap()).unwrap();
    focus::browser_results_red(&report, snapshot)
}

pub(super) fn passing(snapshot: &Path, runs: bool) -> git::Result<focus::CheckReport> {
    let status = if runs { "expected" } else { "skipped" };
    parse(
        snapshot,
        json!({"errors": [], "suites": [{
            "file": "new.spec.ts", "title": "new.spec.ts", "specs": [{
                "file": "new.spec.ts", "title": "works", "line": 2,
                "tests": [{"projectName": "webgpu", "status": status,
                    "expectedStatus": "passed",
                    "results": [{"status": if runs { "passed" } else { "skipped" }}]}]
            }]
        }]}),
    )
}

pub(super) fn branch(add: bool) -> (tempfile::TempDir, String, String) {
    let (temp, base) = repo();
    let root = temp.path();
    let mut files = vec![(SPEC, SOURCE)];
    if add {
        files.push((MODULE, "export function fresh() {}\n"));
    }
    commit(root, &files, "new module and its spec");
    let head = tip(root);
    (temp, base, head)
}

#[test]
fn js_compile_red_added_module_load_errors_name_missing_and_ran() {
    for kind in ["cannot", "url", "node", "copy", "export"] {
        for suite in [false, true] {
            let (temp, base, head) = branch(true);
            let root = temp.path();
            let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
                if snapshot.join(MODULE).exists() {
                    return passing(snapshot, true);
                }
                let module = snapshot.join(MODULE).display().to_string();
                let message = match kind {
                    "cannot" => "Error: Cannot find module '../../web/new.mjs'".into(),
                    "url" => "Error: Failed to load url ../../web/new.mjs (resolved id: ../../web/new.mjs)".into(),
                    "node" => format!("Error [ERR_MODULE_NOT_FOUND]: Cannot find module '{module}' imported from {}", snapshot.join(SPEC).display()),
                    "copy" => format!("Error: ENOENT: no such file or directory, copyfile '{module}' -> '/scratch/new.mjs'"),
                    _ => "SyntaxError: The requested module '../../web/new.mjs' does not provide an export named 'fresh'".into(),
                };
                parse(snapshot, fixture(snapshot, message, SPEC, suite))
            }).unwrap_or_else(|e| panic!("{kind}, suite={suite}: {e}"));
            assert_eq!(
                record.missing,
                [if kind == "export" { "fresh" } else { MODULE }]
            );
            assert_eq!(record.ran, [NAME]);
            assert!(record.failed.is_empty());
            assert!(red::require(root, &base, &head).unwrap().contains(NAME));
        }
    }
}

#[test]
fn js_compile_red_unadded_module_or_export_is_refused() {
    for message in [
        "Cannot find module '../../web/absent.mjs'",
        "The requested module '../../web/new.mjs' does not provide an export named 'absent'",
    ] {
        let (temp, base, head) = branch(true);
        let root = temp.path();
        let mut calls = 0;
        let result = red::run_with(root, &base, &head, &mut |snapshot, _| {
            calls += 1;
            if calls > 1 {
                return passing(snapshot, true);
            }
            parse(snapshot, fixture(snapshot, message.into(), SPEC, false))
        });
        assert!(result.is_err(), "unexplained error accepted: {result:?}");
        assert_eq!(calls, 1, "must refuse the unexplained load at red");
        assert!(red::require(root, &base, &head).is_err());
    }
    let (temp, base, head) = branch(false);
    let mut calls = 0;
    let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
        calls += 1;
        if calls > 1 {
            return passing(snapshot, true);
        }
        parse(
            snapshot,
            fixture(
                snapshot,
                "Cannot find module '../../web/new.mjs'".into(),
                SPEC,
                false,
            ),
        )
    });
    assert!(
        result.is_err(),
        "PR must actually add the module: {result:?}"
    );
    assert_eq!(calls, 1);
}

#[test]
fn js_compile_red_errors_outside_changed_specs_are_refused() {
    for file in [
        "browser/tests/other.spec.ts",
        "browser/playwright.config.ts",
        "web/loader.mjs",
    ] {
        let (temp, base, head) = branch(true);
        let mut calls = 0;
        let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
            calls += 1;
            if calls > 1 {
                return passing(snapshot, true);
            }
            parse(
                snapshot,
                fixture(
                    snapshot,
                    format!("Cannot find module '{}'", snapshot.join(MODULE).display()),
                    file,
                    false,
                ),
            )
        });
        assert!(result.is_err(), "{file}: {result:?}");
        assert_eq!(calls, 1, "outside errors must be refused at red");
    }
}

#[test]
fn js_compile_red_each_spec_must_run_and_pass_at_head() {
    for unrelated in [false, true] {
        let (temp, base, head) = branch(true);
        let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
            if snapshot.join(MODULE).exists() {
                let mut report = passing(snapshot, unrelated)?;
                if unrelated {
                    report
                        .locations
                        .values_mut()
                        .for_each(|(p, _)| *p = "browser/tests/other.spec.ts".into());
                }
                return Ok(report);
            }
            parse(
                snapshot,
                fixture(
                    snapshot,
                    "Cannot find module '../../web/new.mjs'".into(),
                    SPEC,
                    false,
                ),
            )
        });
        let error = result.unwrap_err();
        assert!(error.contains("ran and passed at HEAD"), "{error}");
        assert!(red::require(temp.path(), &base, &head).is_err());
    }
}

#[test]
fn js_compile_red_changed_imported_helper_requires_its_specs_to_run() {
    for imports in [true, false] {
        let (temp, _) = repo();
        let root = temp.path();
        let helper = "browser/tests/helper.ts";
        commit(
            root,
            &[
                (
                    SPEC,
                    if imports {
                        "import './helper';\ntest('works', () => {});\n"
                    } else {
                        "// import './helper';\ntest('works', () => {});\n"
                    },
                ),
                (helper, "// old helper\n"),
            ],
            "old spec and helper",
        );
        let base = tip(root);
        commit(
            root,
            &[
                (helper, "import '../../web/new.mjs';\n"),
                (MODULE, "export const fresh = 1;\n"),
            ],
            "changed helper imports new module",
        );
        let head = tip(root);
        let result = red::run_with(root, &base, &head, &mut |snapshot, _| {
            if snapshot.join(MODULE).exists()
                || !fs::read_to_string(snapshot.join(helper))
                    .unwrap()
                    .contains("import")
            {
                return passing(snapshot, true);
            }
            parse(
                snapshot,
                fixture(
                    snapshot,
                    "Cannot find module '../../web/new.mjs'".into(),
                    helper,
                    true,
                ),
            )
        });
        assert_eq!(result.is_ok(), imports, "{result:?}");
        if imports {
            assert_eq!(result.unwrap().ran, [NAME]);
        }
    }
}

#[test]
fn js_compile_red_unknown_collection_errors_and_incomplete_json_stay_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for message in [
        "SyntaxError: Unexpected token",
        "Error: No tests found.",
        "Error: worker crashed",
        "Error [ERR_MODULE_NOT_FOUND]: unknown module",
    ] {
        assert!(
            parse(root, fixture(root, message.into(), SPEC, false)).is_err(),
            "{message}"
        );
    }
    let mut value = fixture(
        root,
        "Cannot find module '../../web/new.mjs'".into(),
        SPEC,
        false,
    );
    value["errors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"message": "server lost"}));
    assert!(parse(root, value).is_err());
    let file = root.join("report.json");
    fs::write(&file, "{\"errors\": [").unwrap();
    assert!(focus::browser_results_red(&file, root).is_err());
}

#[test]
fn js_compile_red_stack_locations_and_explicit_outside_locations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for stack in [
        format!(
            "Error: Cannot find module '{}' imported from {}",
            root.join(MODULE).display(),
            root.join(SPEC).display()
        ),
        format!(
            "Error: Cannot find module '{}'\nRequire stack:\n- {}",
            root.join(MODULE).display(),
            root.join(SPEC).display()
        ),
        format!(
            "Error: Cannot find module '{}'\n    at {}:1:3",
            root.join(MODULE).display(),
            root.join(SPEC).display()
        ),
    ] {
        let value = json!({"suites": [], "errors": [{"message": stack, "stack": stack}]});
        let report = parse(root, value.clone()).unwrap();
        assert_eq!(report.compile_errors[0].path, SPEC);
        assert_eq!(report.compile_errors[0].items, [MODULE]);
        let mut outside = value;
        outside["errors"][0]["location"] = json!({"file": "/outside/config.ts", "line": 1});
        assert!(
            parse(root, outside).is_err(),
            "explicit outside location takes precedence"
        );
    }
}

#[test]
fn js_compile_red_a_missing_module_already_present_at_base_is_refused() {
    let (temp, _) = repo();
    let root = temp.path();
    commit(
        root,
        &[(MODULE, "export const old = 1;\n")],
        "existing module",
    );
    let base = tip(root);
    commit(
        root,
        &[(SPEC, SOURCE), (MODULE, "export function fresh() {}\n")],
        "edit existing module",
    );
    let mut calls = 0;
    let result = red::run_with(root, &base, &tip(root), &mut |snapshot, _| {
        calls += 1;
        if calls > 1 {
            return passing(snapshot, true);
        }
        parse(
            snapshot,
            fixture(
                snapshot,
                "Cannot find module '../../web/new.mjs'".into(),
                SPEC,
                false,
            ),
        )
    });
    assert!(
        result.is_err(),
        "existing path cannot explain a load failure: {result:?}"
    );
    assert_eq!(calls, 1);
}

#[test]
fn js_compile_red_each_importing_spec_must_run_after_helper_failure() {
    let (temp, _) = repo();
    let root = temp.path();
    let helper = "browser/tests/helper.ts";
    let source = "import './helper';\ntest('works', () => {});\n";
    commit(
        root,
        &[
            (SPEC, source),
            ("browser/tests/other.spec.ts", source),
            (helper, "// old\n"),
        ],
        "two specs",
    );
    let base = tip(root);
    commit(
        root,
        &[
            (helper, "import '../../web/new.mjs';\n"),
            (MODULE, "export const fresh = 1;\n"),
        ],
        "helper needs new module",
    );
    let result = red::run_with(root, &base, &tip(root), &mut |snapshot, _| {
        if snapshot.join(MODULE).exists()
            || !fs::read_to_string(snapshot.join(helper))
                .unwrap()
                .contains("import")
        {
            return passing(snapshot, true);
        }
        parse(
            snapshot,
            fixture(
                snapshot,
                "Cannot find module '../../web/new.mjs'".into(),
                helper,
                false,
            ),
        )
    });
    let error = result.unwrap_err();
    assert!(
        error.contains("other.spec.ts ran and passed at HEAD"),
        "{error}"
    );
}
