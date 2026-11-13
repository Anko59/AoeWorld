use super::balance::{Durations, Result, assign, records};
use std::{fs, path::PathBuf};

#[cfg(test)]
mod tests;

/// Rebuild the current duration weights from disjoint Playwright JSON reports.
#[derive(clap::Args, Debug)]
pub(crate) struct DurationsArgs {
    #[arg(required = true)]
    reports: Vec<PathBuf>,
    #[arg(long, default_value = "crates/harness/src/e2e/shard/durations.json")]
    output: PathBuf,
    /// Also refresh the partition fixture from a real `make e2e-list` JSON report.
    #[arg(long)]
    list: Option<PathBuf>,
}

pub(crate) fn regenerate(options: &DurationsArgs) -> Result<()> {
    let mut milliseconds = Durations::new();
    for report in &options.reports {
        for (name, duration) in records(&serde_json::from_slice(&fs::read(report)?)?)? {
            if milliseconds.insert(name, duration).is_some() {
                return Err("duration reports overlap; supply each shard exactly once".into());
            }
        }
    }
    let seconds: Durations = milliseconds
        .iter()
        .map(|(name, ms)| {
            (
                name.clone(),
                (ms / 1000 + u64::from(ms % 1000 >= 500)).max(1),
            )
        })
        .collect();
    let names: Vec<_> = seconds.keys().cloned().collect();
    let plan = assign(&names, &seconds, 4)?;
    for (index, tests) in plan.iter().enumerate() {
        let ms: u64 = tests.iter().map(|name| milliseconds[name]).sum();
        println!(
            "shard {}/4: {} tests, {:.2} measured minutes",
            index + 1,
            tests.len(),
            ms as f64 / 60_000.0
        );
    }
    let mut output = serde_json::to_string_pretty(&seconds)?;
    output.push('\n');
    fs::write(&options.output, output)?;
    if let Some(list) = &options.list {
        let report: serde_json::Value = serde_json::from_slice(&fs::read(list)?)?;
        records(&report)?;
        // Keep the actual discovery tree, one file suite per line to stay
        // bounded. Machine-local config paths and timings are irrelevant.
        let suites = report["suites"]
            .as_array()
            .ok_or("missing suites")?
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        fs::write(
            "crates/harness/src/e2e/shard/list.json",
            format!(
                "{{\"errors\":[],\"suites\":[\n{}\n]}}\n",
                suites.join(",\n")
            ),
        )?;
    }
    Ok(())
}
