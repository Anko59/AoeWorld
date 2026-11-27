//! The runner that builds and plays for real: the branch with its own release binary, the merge
//! base with the release binary of the scratch worktree (`base`).
use super::{BasePlayed, BoxError, BranchPlayed, Run, Runner, base, launch};
use serde_json::{Value, json};
use std::{fs, path::Path, thread, time::Instant};

pub(super) struct Real;

/// The flags every game score child takes to find the map.
fn map_flags(run: &Run) -> Vec<String> {
    vec![
        "--package-directory".to_owned(),
        run.package_directory.display().to_string(),
        "--map-hash".to_owned(),
        run.map_hash.clone(),
    ]
}

/// The worker counts the branch is played at: 1 against 4, not against every core. Several agents
/// ship at once on one machine, and the determinism check needs two different counts, not the
/// widest one. The numbers of the game do not depend on the count.
pub(super) const WORKERS: &str = "1,4";

/// The flags of the branch's run: every scenario at every count, all at once.
pub(super) fn branch_flags(run: &Run, out: &Path) -> Vec<String> {
    let mut flags = map_flags(run);
    flags.extend([
        "--workers".to_owned(),
        WORKERS.to_owned(),
        "--parallel".to_owned(),
    ]);
    flags.extend(["--out".to_owned(), out.display().to_string()]);
    flags
}

fn read(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// A directory the run writes its reports and order logs into, emptied first so an old file is
/// never read as this run's.
fn scratch(run: &Run, name: &str) -> Result<std::path::PathBuf, BoxError> {
    let directory = run.root.join(".cache/game-score/pr").join(name);
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory)?;
    Ok(directory)
}

impl Runner for Real {
    /// Release-builds this checkout, then plays every scenario at every worker count at once.
    fn play_branch(&self, run: &Run) -> Result<BranchPlayed, BoxError> {
        let directory = scratch(run, "branch")?;
        let built = launch::build(&run.root)?;
        if !built.success {
            return Ok(BranchPlayed {
                report: None,
                crash: Some(format!(
                    "the release build of this branch failed: {}",
                    built.tail_text()
                )),
                build_seconds: built.seconds,
                play_seconds: 0.0,
            });
        }
        let target = launch::target(&run.root);
        let out = directory.join("game-score.json");
        let flags = branch_flags(run, &out);
        let command = launch::game_score(&launch::binary(&target), &run.root, &target, &flags);
        let played = launch::run(command, launch::PLAY_DEADLINE)?;
        let report = read(&out);
        // A run that differs between worker counts still writes its report and says so in it.
        let crash = report.is_none().then(|| {
            format!(
                "the game score of this branch did not finish: {}",
                played.tail_text()
            )
        });
        Ok(BranchPlayed {
            report,
            crash,
            build_seconds: built.seconds,
            play_seconds: played.seconds,
        })
    }

    /// Moves the scratch worktree to the merge base, builds it, and plays each scenario of the
    /// branch in a process of its own, at one worker.
    fn play_base(&self, run: &Run, scenarios: &[String]) -> Result<BasePlayed, BoxError> {
        let checkout = base::prepare(&run.root, &run.home, &run.base)?;
        let built = launch::build(&checkout)?;
        if !built.success {
            return Err(format!("its release build failed: {}", built.tail_text()).into());
        }
        let directory = scratch(run, "base")?;
        let target = launch::target(&checkout);
        let binary = launch::binary(&target);
        let started = Instant::now();
        let results: Vec<Result<Option<Value>, String>> = thread::scope(|scope| {
            let handles: Vec<_> = scenarios
                .iter()
                .map(|id| {
                    let out = directory.join(format!("{id}.json"));
                    let mut flags = map_flags(run);
                    flags.extend(["--scenario".to_owned(), id.clone()]);
                    flags.extend(["--workers".to_owned(), "1".to_owned(), "--out".to_owned()]);
                    flags.push(out.display().to_string());
                    let command = launch::game_score(&binary, &checkout, &target, &flags);
                    scope.spawn(move || -> Result<Option<Value>, String> {
                        let finished = launch::run(command, launch::PLAY_DEADLINE)
                            .map_err(|error| error.to_string())?;
                        if let Some(report) = read(&out) {
                            return Ok(Some(report));
                        }
                        // A scenario the merge base does not have is no reason to refuse it.
                        if finished
                            .tail
                            .iter()
                            .any(|line| line.contains("no scenario"))
                        {
                            return Ok(None);
                        }
                        Err(finished.tail_text())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err("a game score child panicked".to_owned()))
                })
                .collect()
        });
        let mut reports = Vec::new();
        for result in results {
            match result {
                Ok(Some(report)) => reports.push(report),
                Ok(None) => {}
                Err(tail) => return Err(format!("playing it failed: {tail}").into()),
            }
        }
        if reports.is_empty() {
            return Err("it has none of the scenarios of this branch".into());
        }
        Ok(BasePlayed {
            report: merge(reports),
            build_seconds: built.seconds,
            play_seconds: started.elapsed().as_secs_f64(),
        })
    }
}

/// The scenarios of the reports that played, in one report with the first one's header.
pub(super) fn merge(reports: Vec<Value>) -> Value {
    let header = reports
        .first()
        .map_or(Value::Null, |report| report["header"].clone());
    let scenarios: Vec<Value> = reports
        .iter()
        .flat_map(|report| report["scenarios"].as_array().cloned().unwrap_or_default())
        .collect();
    json!({ "schema": super::super::report::SCHEMA, "name": "game score", "header": header, "scenarios": scenarios })
}
