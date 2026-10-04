use super::*;
use std::collections::BTreeMap;
#[derive(Debug)]
pub(super) struct Hunk {
    pub(super) path: String,
    pub(super) removed: Vec<String>,
    pub(super) added: Vec<String>,
}
#[derive(Debug, Serialize)]
pub(super) struct Finding {
    pub(super) disposition: &'static str,
    pub(super) reasons: BTreeSet<String>,
    pub(super) independent_review_required: bool,
    pub(super) authoritative: bool,
}
/// Complete bounded immutable UTF-8 diff comes from the caller's Git comparison.
/// String heuristics are review tripwires, NOT AST/test quality or role proof.
pub(super) fn inspect(registry: &Registry, paths: &[String], hunks: &[Hunk]) -> Result<Finding> {
    if paths.len() > 4096 || hunks.len() > 4096 {
        return Err("diff count exceeds planning bound".into());
    }
    let mut bytes = 0usize;
    let mut reasons = BTreeSet::new();
    let classification = registry.classify(paths);
    if registry
        .suites
        .iter()
        .any(|suite| suite.review && classification.suites.contains(&suite.id))
    {
        reasons.insert("Protected/unknown changes require independent policy review".into());
    }
    for path in paths {
        if !relative(path) {
            return Err("invalid immutable diff path".into());
        }
        if test_path(path) {
            reasons.insert(format!("Test ownership changed: {path}"));
        }
    }
    for hunk in hunks {
        if !paths.contains(&hunk.path)
            || !relative(&hunk.path)
            || hunk.added.len() + hunk.removed.len() > 4096
        {
            return Err("hunk must belong to resolved diff and be bounded".into());
        }
        for line in hunk.added.iter().chain(&hunk.removed) {
            if line.len() > 4096 || line.chars().any(|ch| ch.is_control() && ch != '\t') {
                return Err("diff line exceeds bound or contains unsupported control bytes".into());
            }
            bytes = bytes
                .checked_add(line.len())
                .ok_or("diff length overflow")?;
            if bytes > 8 * 1024 * 1024 {
                return Err(
                    "complete immutable diff exceeds 8MiB; independent manual review required"
                        .into(),
                );
            }
        }
        for line in &hunk.added {
            if [
                "#[ignore",
                "cfg(ignore",
                "should_panic",
                "test.skip",
                "test.only",
                "describe.only",
                "it.only",
                ".skip(",
                "#[cfg(test)]",
                "#[test]",
                "proptest_config",
                "cases",
                "coverage",
                "threshold",
                "budget",
                "required",
                "campaign",
                "assertions",
                "counter",
            ]
            .iter()
            .any(|needle| line.contains(needle))
            {
                reasons.insert(format!(
                    "Potential ignored/narrowed/changed test selection or counter: {}",
                    hunk.path
                ));
            }
        }
        for line in &hunk.removed {
            if [
                "assert!",
                "assert_eq!",
                "assert_ne!",
                "ensure!",
                "expect(",
                "#[test]",
                "proptest!",
                "test(",
                "cases",
                "coverage",
                "threshold",
                "budget",
                "required",
                "campaign",
                "assertions",
                "counter",
            ]
            .iter()
            .any(|needle| line.contains(needle))
            {
                reasons.insert(format!(
                    "Potential deleted assertion/test/campaign bound: {}",
                    hunk.path
                ));
            }
        }
        let old = counters(&hunk.removed);
        let new = counters(&hunk.added);
        for (key, before) in old {
            if let Some(after) = new.get(&key)
                && before
                    .iter()
                    .zip(after)
                    .any(|(before, after)| after < before)
            {
                reasons.insert(format!(
                    "Potential numeric test/campaign counter decrease ({key}): {}",
                    hunk.path
                ));
            }
        }
    }
    Ok(Finding {
        disposition: if reasons.is_empty() {
            "REVIEW_NOT_ASSESSED"
        } else {
            "REVIEW_REQUIRED"
        },
        reasons,
        independent_review_required: true,
        authoritative: false,
    })
}
fn test_path(path: &str) -> bool {
    path == "tests.rs"
        || path.ends_with("/tests.rs")
        || path.contains("/tests/")
        || path.starts_with("tests/")
        || path.contains(".test.")
        || path.contains(".spec.")
        || path.split('/').any(|part| part == "test")
}
fn counters(lines: &[String]) -> BTreeMap<String, Vec<f64>> {
    let mut result = BTreeMap::<String, Vec<f64>>::new();
    for line in lines {
        let tokens: Vec<_> = line
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
            .filter(|token| !token.is_empty())
            .collect();
        for pair in tokens.windows(2) {
            let key = pair[0].to_ascii_lowercase();
            if ![
                "case",
                "test",
                "assert",
                "counter",
                "campaign",
                "coverage",
                "threshold",
                "budget",
                "iteration",
                "required",
                "timeout",
                "runs",
            ]
            .iter()
            .any(|name| key.contains(name))
            {
                continue;
            }
            let number = pair[1].replace('_', "");
            if let Ok(value) = number.parse::<f64>()
                && value.is_finite()
            {
                result.entry(key).or_default().push(value);
            }
        }
    }
    result
}
