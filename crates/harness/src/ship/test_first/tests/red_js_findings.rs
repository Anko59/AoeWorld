//! Further attribution boundaries of JavaScript compile-red.
use super::{commit, red_js::*, repo};
use crate::ship::{duel::focus, test_first::red};
use serde_json::json;
use std::fs;

#[test]
fn js_compile_red_green_requires_an_actual_passing_test() {
    let cases = [
        json!({"status": "expected", "expectedStatus": "failed", "results": [{"status": "failed"}]}),
        json!({"status": "expected", "expectedStatus": "skipped", "annotations": [{"type": "fixme"}], "results": [{"status": "skipped"}]}),
        json!({"status": "skipped", "expectedStatus": "passed", "results": [{"status": "skipped"}]}),
        json!({"status": "expected", "expectedStatus": "passed", "results": [{"status": "failed"}]}),
        json!({"status": "expected", "expectedStatus": "passed", "results": []}),
        json!({"status": "expected", "results": [{"status": "passed"}]}),
    ];
    for test in cases {
        let (temp, base, head) = branch(true);
        let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
            if snapshot.join(MODULE).exists() {
                let mut test = test.clone();
                test["projectName"] = json!("webgpu");
                return parse(
                    snapshot,
                    json!({"errors": [], "suites": [{"specs": [{
                        "file": "new.spec.ts", "title": "works", "line": 2, "tests": [test]
                    }]}]}),
                );
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
        let error = result.expect_err(&format!("non-passing test earned proof: {test}"));
        assert!(error.contains("ran and passed at HEAD"), "{test}: {error}");
        assert!(red::require(temp.path(), &base, &head).is_err());
    }
}

#[test]
fn js_compile_red_only_actual_passes_are_recorded_in_a_mixed_spec() {
    let (temp, base, _) = branch(true);
    commit(
        temp.path(),
        &[(
            SPEC,
            &format!(
                "{SOURCE}test('expected failure', () => {{ test.fail(); throw new Error('expected'); }});\n"
            ),
        )],
        "also an expected failure",
    );
    let head = tip(temp.path());
    let record = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
        if snapshot.join(MODULE).exists() {
            return parse(
                snapshot,
                json!({"errors": [], "suites": [{"specs": [
                    {"file": "new.spec.ts", "title": "works", "line": 2, "tests": [{
                        "projectName": "webgpu", "status": "expected", "expectedStatus": "passed",
                        "results": [{"status": "passed"}]
                    }]},
                    {"file": "new.spec.ts", "title": "expected failure", "line": 3, "tests": [{
                        "projectName": "webgpu", "status": "expected", "expectedStatus": "failed",
                        "results": [{"status": "failed"}]
                    }]}
                ]}]}),
            );
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
    })
    .unwrap();
    assert_eq!(record.ran, ["new.spec.ts::works [webgpu]"]);
    assert!(red::require(temp.path(), &base, &head).is_ok());
}

#[test]
fn js_compile_red_typescript_helpers_resolve_javascript_specifiers() {
    for (specifier, extension) in [("js", "ts"), ("mjs", "mts"), ("cjs", "cts")] {
        for transitive in [false, true] {
            let (temp, _) = repo();
            let root = temp.path();
            let helper = format!("browser/tests/helper.{extension}");
            let import = format!("import './helper.{specifier}';\n");
            let spec = format!(
                "{}test('works', () => {{}});\n",
                if transitive {
                    "import './bridge.js';\n"
                } else {
                    &import
                }
            );
            commit(
                root,
                &[
                    (SPEC, &spec),
                    (&helper, "// old helper\n"),
                    ("browser/tests/bridge.ts", &import),
                ],
                "spec imports TS helper",
            );
            let base = tip(root);
            commit(
                root,
                &[
                    (&helper, "import '../../web/new.mjs';\n"),
                    (MODULE, "export const fresh = 1;\n"),
                ],
                "helper needs added module",
            );
            let head = tip(root);
            let result = red::run_with(root, &base, &head, &mut |snapshot, _| {
                if snapshot.join(MODULE).exists()
                    || !fs::read_to_string(snapshot.join(&helper))
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
                        &helper,
                        false,
                    ),
                )
            });
            let record = result.unwrap_or_else(|error| {
                panic!(".{specifier} -> .{extension}, transitive={transitive}: {error}")
            });
            assert_eq!(record.missing, [MODULE]);
            assert_eq!(record.ran, ["new.spec.ts::works [webgpu]"]);
            assert!(red::require(root, &base, &head).is_ok());
        }
    }
}

#[test]
fn js_compile_red_playwright_anonymous_eval_uses_the_named_importer() {
    let (temp, base, head) = branch(true);
    let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
        if snapshot.join(MODULE).exists() {
            return passing(snapshot, true);
        }
        let message = format!(
            "Error: Cannot find module '{}' imported from {}",
            snapshot.join(MODULE).display(),
            snapshot.join(SPEC).display()
        );
        let mut value = fixture(snapshot, message.clone(), SPEC, false);
        value["errors"][0]["stack"] = json!(format!("{message}\n    at eval (<anonymous>:1:1)"));
        value["errors"][0]["location"]["file"] = json!("<anonymous>");
        parse(snapshot, value)
    });
    let record =
        result.unwrap_or_else(|error| panic!("actual Playwright ESM collection shape: {error}"));
    assert_eq!(record.missing, [MODULE]);
    assert_eq!(record.ran, ["new.spec.ts::works [webgpu]"]);

    let root = temp.path();
    let mut unknown = fixture(
        root,
        format!("Cannot find module '{}'", root.join(MODULE).display()),
        SPEC,
        true,
    );
    unknown["suites"][0]["errors"][0]["location"]["file"] = json!("<anonymous>");
    unknown["suites"][0]["errors"][0]["stack"] = json!("Error\n    at eval (<anonymous>:1:1)");
    assert!(
        parse(root, unknown).is_err(),
        "anonymous eval without a named importer is unavailable"
    );
}

#[test]
fn js_compile_red_does_not_explain_unchanged_helpers_but_accepts_changed_imports() {
    for changed in [true, false] {
        let (temp, _) = repo();
        let root = temp.path();
        let helper = "web/helper.mjs";
        let old = "import './new.mjs';\nexport function fresh() {}\n";
        commit(root, &[(helper, old)], "existing helper");
        let base = tip(root);
        let spec =
            "import { fresh } from '../../web/helper.mjs';\ntest('works', () => { fresh(); });\n";
        let mut files = vec![(SPEC, spec), (MODULE, "export const available = true;\n")];
        if changed {
            files.push((
                helper,
                "import './new.mjs';\nexport function fresh() { return true; }\n",
            ));
        }
        commit(root, &files, "new module and spec");
        let result = red::run_with(root, &base, &tip(root), &mut |snapshot, _| {
            if snapshot.join(MODULE).exists() {
                return passing(snapshot, true);
            }
            parse(
                snapshot,
                fixture(
                    snapshot,
                    "Cannot find module './new.mjs'".into(),
                    helper,
                    false,
                ),
            )
        });
        assert_eq!(result.is_ok(), changed, "changed={changed}: {result:?}");
    }
}

#[test]
fn js_compile_red_new_export_of_existing_module_including_dollar_names() {
    for item in ["fresh", "$fresh"] {
        let (temp, _) = repo();
        let root = temp.path();
        commit(root, &[(MODULE, "export const old = 1;\n")], "old module");
        let base = tip(root);
        let product = format!("export function {item}() {{}}\n");
        let spec = SOURCE.replace("fresh", item);
        commit(
            root,
            &[(SPEC, &spec), (MODULE, &product)],
            "new export and spec",
        );
        let result = red::run_with(root, &base, &tip(root), &mut |snapshot, _| {
            if fs::read_to_string(snapshot.join(MODULE))
                .unwrap()
                .contains(item)
            {
                return passing(snapshot, true);
            }
            parse(
                snapshot,
                fixture(
                    snapshot,
                    format!(
                        "The requested module '../../web/new.mjs' does not provide an export named '{item}'"
                    ),
                    SPEC,
                    false,
                ),
            )
        });
        assert_eq!(result.unwrap().missing, [item]);
    }
}

#[test]
fn js_compile_red_does_not_explain_an_unrelated_rust_compile_failure() {
    let (temp, base, _) = branch(true);
    let root = temp.path();
    let tests = "crates/sim/src/tests.rs";
    commit(
        root,
        &[(tests, "#[test]\nfn unrelated() {}\n")],
        "unrelated tests",
    );
    let mut calls = 0;
    let result = red::run_with(root, &base, &tip(root), &mut |snapshot, _| {
        calls += 1;
        if snapshot.join(MODULE).exists() {
            return passing(snapshot, true);
        }
        let mut report = parse(
            snapshot,
            fixture(
                snapshot,
                "Cannot find module '../../web/new.mjs'".into(),
                SPEC,
                false,
            ),
        )?;
        report.compile_errors.push(focus::CompileError {
            path: tests.into(),
            code: Some("E0308".into()),
            items: vec![],
        });
        Ok(report)
    });
    assert!(
        result.is_err(),
        "JS miss cannot explain unrelated Rust build errors: {result:?}"
    );
    assert_eq!(calls, 1, "unexplained Rust errors must be refused at red");
}

#[test]
fn js_compile_red_does_not_explain_an_outside_stack_by_suite_file() {
    let (temp, base, head) = branch(true);
    let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
        if snapshot.join(MODULE).exists() {
            return passing(snapshot, true);
        }
        let mut value = fixture(
            snapshot,
            "The requested module '../../web/new.mjs' does not provide an export named 'fresh'"
                .into(),
            SPEC,
            true,
        );
        let error = &mut value["suites"][0]["errors"][0];
        error.as_object_mut().unwrap().remove("location");
        error["stack"] = json!("SyntaxError\n    at /outside/config.ts:1:1");
        parse(snapshot, value)
    });
    assert!(
        result.is_err(),
        "suite fallback hid an outside error: {result:?}"
    );
}

#[test]
fn js_compile_red_does_not_explain_nameless_panics() {
    let (temp, base, head) = branch(true);
    let result = red::run_with(temp.path(), &base, &head, &mut |snapshot, _| {
        if snapshot.join(MODULE).exists() {
            return passing(snapshot, true);
        }
        let mut report = parse(
            snapshot,
            fixture(
                snapshot,
                "Cannot find module '../../web/new.mjs'".into(),
                SPEC,
                false,
            ),
        )?;
        report.panics.push(focus::Panic {
            path: "crates/sim/src/tests.rs".into(),
            location: "crates/sim/src/tests.rs:1:1".into(),
            message: "nameless runner panic".into(),
            test: None,
        });
        Ok(report)
    });
    assert!(
        result.is_err(),
        "load error hid a nameless panic: {result:?}"
    );
}
