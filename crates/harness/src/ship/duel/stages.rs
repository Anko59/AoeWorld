//! Native tests keep test-unit's image; only the browser adapter needs Docker CLI.
use super::{
    baseline,
    focus::{self, CheckReport, Scope},
};
use crate::ship::git;
use std::{fs, path::Path};

pub(crate) fn native(root: &Path) -> git::Result<()> {
    let path = root.join(".cache/duel/check.json");
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let result = focus::execute(root);
    if result.is_err() && path.exists() {
        let report: CheckReport =
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if report.unavailable.is_none() {
            return Ok(());
        }
    }
    result
}

pub(crate) fn browser(root: &Path) -> git::Result<()> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let cache = root.join(".cache/duel");
    let mut report: CheckReport =
        serde_json::from_slice(&fs::read(cache.join("check.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut scope: Scope =
        serde_json::from_slice(&fs::read(cache.join("scope.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if !scope.specs.is_empty() || !scope.wasm.is_empty() {
        scope.packages.clear();
        let revision = std::env::var("HARNESS_DUEL_REVISION")
            .ok()
            .filter(|s| !s.is_empty());
        let temporary;
        let check_root = if let Some(revision) = revision.as_deref() {
            temporary = baseline::export(&root, revision)?;
            temporary.path()
        } else {
            &root
        };
        let checked = focus::run_scope(check_root, &scope, &cache);
        let mut browser = match checked {
            Ok(report) => report,
            Err(error) => CheckReport {
                build_failed: error.starts_with("build failed:"),
                unavailable: (!error.starts_with("build failed:")).then_some(error.clone()),
                failed: vec![error],
                ..Default::default()
            },
        };
        if revision.is_none() {
            let base =
                std::env::var("HARNESS_DUEL_BASE").map_err(|_| "HARNESS_DUEL_BASE is required")?;
            focus::classify_specs(&root, &base, &mut browser)?;
            if !browser.build_failed {
                let wasm: Vec<_> = browser
                    .failed
                    .iter()
                    .filter(|n| !browser.locations.contains_key(*n))
                    .cloned()
                    .collect();
                browser
                    .untouched
                    .extend(baseline::untouched(&root, &base, &scope, &wasm)?);
            }
        }
        report.passed &= browser.passed;
        report.build_failed |= browser.build_failed;
        report.unavailable = report.unavailable.or(browser.unavailable);
        report.tests.extend(browser.tests);
        report.failed.extend(browser.failed);
        report.untouched.extend(browser.untouched);
        report.locations.extend(browser.locations);
    }
    focus::save(&cache, &report)?;
    if let Some(reason) = report.unavailable {
        Err(format!("check could not complete: {reason}"))
    } else if report.passed {
        Ok(())
    } else {
        Err(format!(
            "focused tests failed: {}",
            report.failed.join(", ")
        ))
    }
}
