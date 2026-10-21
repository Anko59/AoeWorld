//! Fail closed on incomplete execution, setup/teardown and runner errors.
use super::{Result, string};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct Evidence<'a> {
    results: &'a Value,
    selected: BTreeSet<&'a str>,
    completed: BTreeSet<String>,
}

impl<'a> Evidence<'a> {
    pub(super) fn new(report: &'a Value) -> Result<Self> {
        let evidence = &report["config"]["metadata"]["aoeQuarantine"];
        if !matches!(string(evidence, "status")?, "passed" | "failed") {
            return Err("Playwright run did not complete normally".into());
        }
        let selected = evidence["selected"]
            .as_array()
            .ok_or("missing Playwright selected tests")?;
        let ids = selected
            .iter()
            .map(|id| id.as_str().ok_or("invalid selected test ID"))
            .collect::<std::result::Result<BTreeSet<_>, _>>()?;
        if ids.len() != selected.len() {
            return Err("duplicate Playwright selected tests".into());
        }
        Ok(Self {
            results: &evidence["results"],
            selected: ids,
            completed: BTreeSet::new(),
        })
    }

    pub(super) fn case(&mut self, test: &Value, unexpected: bool, timed_out: bool) -> Result<()> {
        let annotations = test["annotations"]
            .as_array()
            .ok_or("missing test identity")?;
        let ids: Vec<_> = annotations
            .iter()
            .filter(|annotation| annotation["type"] == "aoe-quarantine-id")
            .collect();
        let [identity] = ids.as_slice() else {
            return Err("missing/duplicate Playwright test identity".into());
        };
        let id = string(identity, "description")?;
        if !self.selected.contains(id) || !self.completed.insert(id.to_owned()) {
            return Err("unexpected/duplicate Playwright result".into());
        }
        let result = &self.results[id];
        let errors = result["errors"]
            .as_array()
            .ok_or("missing raw test errors")?;
        if timed_out {
            // A completed timeout is diagnostic evidence, never recovery evidence.
            result["steps"]
                .as_array()
                .ok_or("missing Playwright step evidence")?;
            return Ok(());
        }
        let mut assertions = Vec::new();
        collect_steps(&result["steps"], &mut assertions, false)?;
        if unexpected
            && (errors.is_empty()
                || errors.iter().any(|error| {
                    !assertions
                        .iter()
                        .any(|assertion| same_error(error, assertion))
                }))
        {
            return Err("Playwright failure is not a named test assertion".into());
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<()> {
        if self.selected.len() != self.completed.len() {
            return Err("not every selected Playwright test completed".into());
        }
        Ok(())
    }
}

fn same_error(error: &Value, assertion: &Value) -> bool {
    // The built-in reporter adds snippets to result errors, but not step errors.
    ["message", "stack"].iter().all(|key| {
        error[*key].as_str().is_some_and(|text| !text.is_empty()) && error[*key] == assertion[*key]
    })
}

fn collect_steps<'a>(
    steps: &'a Value,
    assertions: &mut Vec<&'a Value>,
    in_hook: bool,
) -> Result<()> {
    for step in steps.as_array().ok_or("missing Playwright step evidence")? {
        let category = string(step, "category")?;
        let is_hook = matches!(category, "hook" | "fixture");
        let in_hook = in_hook || is_hook;
        let error = &step["error"];
        if !error.is_null() {
            if is_hook {
                return Err("Playwright setup/teardown failure is not recoverable".into());
            }
            if category == "expect" && !in_hook {
                assertions.push(error);
            }
        }
        // Inspect descendants even when the outer hook has no error itself.
        collect_steps(&step["steps"], assertions, in_hook)?;
    }
    Ok(())
}
