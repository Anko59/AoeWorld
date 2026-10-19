//! `make ledger` and `make usage` (docs/models.md): which model did what with
//! what measured result, and what is left of each provider's allowance. The
//! ledger comes from merged pull request descriptions (the one record that
//! exists for every pull request, whichever machine shipped it) and dev's
//! history; usage comes from local data and the providers' own endpoints.
mod fetch;
mod parse;
mod record;
mod table;
mod usage;

#[cfg(test)]
mod tests;

use fetch::{Gh, Refresh, Source};
use record::{Ledger, Pr};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Newest merged pull requests the ledger covers.
const WINDOW: usize = 200;
/// GitHub API calls one run may make (the listing included).
const MAX_CALLS: usize = 50;
const CACHE: &str = ".cache/ledger/ledger.json";

#[derive(clap::Subcommand)]
pub(crate) enum Commands {
    /// Refresh the model ledger from merged pull requests and print it per model.
    Ledger,
    /// Print what is left of each provider's allowance and the last 7 days of the ledger.
    Usage,
}

pub(crate) fn execute(command: Commands) -> Result<()> {
    let root = PathBuf::from(".");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match command {
        Commands::Ledger => {
            let (ledger, note) = refreshed(&root, &Gh(&root));
            println!("{note}");
            let all: Vec<&Pr> = ledger.prs.iter().collect();
            print!("{}", tables("Model ledger", &all, &reverts(&root)));
        }
        Commands::Usage => {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            print!("{}", usage::report(&home, now));
            let (ledger, note) = refreshed(&root, &Gh(&root));
            println!("\n{note}");
            let since = usage::iso(now.saturating_sub(usage::WINDOW_DAYS * 86_400));
            let recent: Vec<&Pr> = ledger
                .prs
                .iter()
                .filter(|pr| pr.merged_at >= since)
                .collect();
            let title = format!("Model ledger, pull requests merged since {since}");
            print!("{}", tables(&title, &recent, &reverts(&root)));
        }
    }
    Ok(())
}

fn limit(variable: &str, default: usize) -> usize {
    std::env::var(variable)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The cached ledger brought up to date within the call budget, and a line
/// saying what that took.
fn refreshed(root: &Path, source: &dyn Source) -> (Ledger, String) {
    let path = root.join(CACHE);
    let mut ledger = Ledger::load(&path);
    let (window, budget) = (
        limit("LEDGER_WINDOW", WINDOW),
        limit("LEDGER_MAX_CALLS", MAX_CALLS),
    );
    let done = fetch::refresh(source, &mut ledger, window, budget);
    let saved = match ledger.save(&path) {
        Ok(()) => format!("cached in {CACHE}"),
        Err(error) => format!("not cached: {error}"),
    };
    (ledger, summary(&done, window, budget, &saved))
}

fn summary(done: &Refresh, window: usize, budget: usize, saved: &str) -> String {
    let mut line = format!(
        "ledger: {} pull request(s) fetched in {} of {budget} API call(s) (newest {window} merged into dev; {saved})",
        done.fetched, done.calls
    );
    if done.missing > 0 {
        line.push_str(&format!(
            "; {} older one(s) wait for the next run (LEDGER_MAX_CALLS)",
            done.missing
        ));
    }
    if let Some(reason) = &done.stopped {
        line.push_str(&format!(
            "\nledger: unknown: GitHub was not fully read: {reason}"
        ));
    }
    line
}

fn reverts(root: &Path) -> Option<BTreeSet<u64>> {
    fetch::dev_subjects(root)
        .ok()
        .map(|subjects| parse::reverted_numbers(&subjects))
}

fn tables(title: &str, prs: &[&Pr], reverted: &Option<BTreeSet<u64>>) -> String {
    let none = BTreeSet::new();
    let known = reverted.as_ref().unwrap_or(&none);
    let mut out = table::render(
        &format!("{title}: by implementer model ({} PRs)", prs.len()),
        "implementer",
        &table::by_implementer(prs, known),
    );
    out.push('\n');
    out.push_str(&table::render(
        "By reviewer model",
        "reviewer",
        &table::by_reviewer(prs, known),
    ));
    out.push_str(
        "\nA pull request counts for every model on it. 'first grade' and 'first pass' come from the review badge:\n\
         a PR that needed several rounds keeps only its last grade, so its first grade is unknown. 'confirmed' is\n\
         the full review's confirmed findings (unknown for PRs whose description shows only a closing review).\n",
    );
    if reverted.is_none() {
        out.push_str("reverted: unknown: dev's history could not be read\n");
    }
    out
}
