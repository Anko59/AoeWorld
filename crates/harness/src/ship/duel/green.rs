//! Green must retain successful execution of the tests that justified red.
use super::{authored, focus::CheckReport, specs};
use crate::ship::git;
use std::{fs, path::Path};

pub(in crate::ship) fn validate(
    root: &Path,
    paths: &[String],
    base: Option<&str>,
    red: &CheckReport,
    green: &CheckReport,
) -> git::Result<()> {
    let passed: Vec<_> = green
        .tests
        .iter()
        .filter(|n| !green.failed.contains(n))
        .collect();
    let mut missing = Vec::new();
    // A red that only panicked names no test: the tester's own tests must be seen running instead.
    if !red.build_failed && (!red.failed.is_empty() || red.panics.is_empty()) {
        // Only the tester's own failures must turn green: a failure the red
        // check marked untouched (a test the tester did not change) is
        // pre-existing and reported as such, not required here.
        missing.extend(
            red.failed
                .iter()
                .filter(|n| !red.untouched.contains(n) && !passed.contains(n))
                .cloned(),
        );
    } else {
        let mut defined = 0;
        for path in paths {
            if !path.ends_with(".rs") && !path.ends_with(".spec.ts") && !path.ends_with(".test.ts")
            {
                continue;
            }
            let unidentified = |error| format!("the tester's tests did not run: {path} ({error})");
            let source =
                fs::read_to_string(root.join(path)).map_err(|e| unidentified(e.to_string()))?;
            if path.ends_with(".rs") {
                let functions = authored::functions(&source).map_err(unidentified)?;
                // Only what this round added or changed must run: untouched, ignored or
                // cfg-gated tests of the same file are not the tester's.
                let old =
                    base.and_then(|base| git::git(root, &["show", &format!("{base}:{path}")]).ok());
                let required =
                    authored::added_functions(&source, old.as_deref()).map_err(unidentified)?;
                defined += required.len();
                for function in &required {
                    let mut ran = false;
                    for name in &passed {
                        let reported = name
                            .split_once('$')
                            .or_else(|| name.split_once(' '))
                            .map_or(name.as_str(), |(_, f)| f.trim());
                        let matched = functions
                            .iter()
                            .filter(|f| {
                                reported == f.as_str() || reported.ends_with(&format!("::{f}"))
                            })
                            .max_by_key(|f| f.len());
                        if matched != Some(function) {
                            continue;
                        }
                        let candidate = CheckReport {
                            failed: vec![(*name).clone()],
                            ..Default::default()
                        };
                        let (matched, notes) =
                            authored::evidence(root, std::slice::from_ref(path), &candidate)?;
                        if matched && notes.is_empty() {
                            ran = true;
                            break;
                        }
                    }
                    if !ran {
                        missing.push(format!("{path}::{function}"));
                    }
                }
            } else {
                let definitions = specs::definitions(&source)
                    .ok_or_else(|| unidentified("cannot identify tests".into()))?;
                defined += definitions.len();
                for (title, line) in &definitions {
                    let unique_line = definitions.iter().filter(|(_, l)| l == line).count() == 1;
                    let literal = ['\'', '"']
                        .into_iter()
                        .find_map(|quote| title.strip_prefix(quote)?.strip_suffix(quote));
                    if !passed.iter().any(|name| {
                        green
                            .locations
                            .get(*name)
                            .is_some_and(|(p, l)| p == path && l == line)
                            && (unique_line
                                || literal.is_some_and(|title| {
                                    name.rsplit_once(" [").is_some_and(|(test, _)| {
                                        test.ends_with(&format!("::{title}"))
                                    })
                                }))
                    }) {
                        missing.push(format!("{path}::{title}"));
                    }
                }
            }
        }
        if defined == 0 {
            missing.extend_from_slice(paths);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the tester's tests did not run: {}",
            missing.join(", ")
        ))
    }
}
