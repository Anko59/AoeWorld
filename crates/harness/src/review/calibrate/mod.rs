//! `make review-calibrate`: run the normal review on every seed case (or a
//! sample) and measure what it finds. A tool for tuning prompts, personas and
//! models, never a gate; the real review is not changed by it.
mod repo;
mod seeds;
mod summary;
#[cfg(test)]
mod tests;

use super::{Report, Tier, config::Config, config::runtime_key, entry, trusted};
use crate::agents::Runtime;
use seeds::Case;
use std::{fs, path::Path};
use summary::{Outcome, Summary};

/// Where the seed cases live.
pub(super) const SEEDS: &str = "gates/review/seeds";

/// Review one case's throwaway repository.
type Reviewer<'a> = &'a dyn Fn(&Path, &Case) -> Result<Report, String>;

/// Review each case in a repository of its own, sequentially (every review is
/// already a burst of parallel sessions). A case that cannot run is a result
/// ("no result"), never a stop.
fn run(source: &Path, cases: &[Case], review: Reviewer) -> Vec<Outcome> {
    cases
        .iter()
        .enumerate()
        .map(|(i, case)| {
            eprintln!("calibrate {}/{}: {}", i + 1, cases.len(), case.id);
            let outcome = repo::materialise(source, case)
                .and_then(|dir| review(dir.path(), case))
                .map_or_else(
                    |error| Outcome::failed(case, error),
                    |report| Outcome::from_report(case, &report),
                );
            eprintln!(
                "  {}",
                outcome.error.as_deref().map_or_else(
                    || format!("grade {}, found {}", outcome.grade, outcome.found),
                    |e| format!("no result: {e}")
                )
            );
            outcome
        })
        .collect()
}

/// The run `make review-calibrate` asks for: CALIBRATE_TIER (default low),
/// CALIBRATE_RUNTIME (default claude) and CALIBRATE_CASES (default all).
pub(super) fn from_env(root: &Path) -> Result<(), String> {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let (tier, runtime, limit) = parse(
        env("CALIBRATE_TIER"),
        env("CALIBRATE_RUNTIME"),
        env("CALIBRATE_CASES"),
    )?;
    write_run(root, tier, runtime, limit, &|dir, case| {
        entry::review_isolated(dir, tier, runtime, &case.task).map_err(|e| e.to_string())
    })
}

fn parse(
    tier: Option<String>,
    runtime: Option<String>,
    cases: Option<String>,
) -> Result<(Tier, Runtime, Option<usize>), String> {
    let tier = match tier {
        Some(name) => clap::ValueEnum::from_str(&name, true)
            .map_err(|_| format!("CALIBRATE_TIER={name}: use low, medium, high, xhigh or max"))?,
        None => Tier::Low,
    };
    let runtime = match runtime {
        Some(name) => clap::ValueEnum::from_str(&name, true)
            .map_err(|_| format!("CALIBRATE_RUNTIME={name}: use claude, codex, dsh or pi"))?,
        None => Runtime::Claude,
    };
    let limit = match cases {
        Some(n) => Some(n.parse::<usize>().ok().filter(|n| *n > 0).ok_or(format!(
            "CALIBRATE_CASES={n}: use a positive number of cases"
        ))?),
        None => None,
    };
    Ok((tier, runtime, limit))
}

/// Run the seeds at `tier` with `review`; write
/// `reports/review/calibration.json` and `.md` under `root` and print the table.
fn write_run(
    root: &Path,
    tier: Tier,
    runtime: Runtime,
    limit: Option<usize>,
    review: Reviewer,
) -> Result<(), String> {
    let cases = seeds::select(seeds::load(&root.join(SEEDS))?, limit);
    if cases.is_empty() {
        return Err("no seed cases selected".into());
    }
    let config = Config::parse(&trusted(root, "gates/review.json")?)?;
    let model = config.model_for(tier, runtime, false);
    let outcomes = run(root, &cases, review);
    let summary = Summary::new(
        tier.name(),
        runtime_key(runtime),
        &format!("{} {}", model.model, model.effort),
        config.merge_grade,
        outcomes,
    );
    let out = root.join("reports/review");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(out.join("calibration.json"), json).map_err(|e| e.to_string())?;
    let markdown = summary.markdown();
    fs::write(out.join("calibration.md"), &markdown).map_err(|e| e.to_string())?;
    println!("{markdown}");
    Ok(())
}
