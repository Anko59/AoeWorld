//! The whole description check (docs/shipping.md): every problem of the
//! body, level, video and title in one list, found before anything slow runs.
//! `make ship` runs it ahead of the lock wait, the gates and the review;
//! `make describe-check` runs it alone.
#[cfg(test)]
mod tests;

use super::{
    Options,
    describe::{Template, level_problems},
    git, github,
    levels::{Change, Decision, Levels},
};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

/// What to check: the description file and the optional title and video.
pub(super) struct Request<'a> {
    pub(super) body: &'a Path,
    pub(super) title: Option<&'a str>,
    /// A new pull request needs a title; an open one keeps its own.
    pub(super) title_required: bool,
    pub(super) video: Option<&'a Path>,
    pub(super) base: &'a str,
    pub(super) fetch: bool,
}

/// The description the check accepted and the level decision to record.
#[derive(Debug)]
pub(super) struct Checked {
    pub(super) text: String,
    pub(super) decision: Decision,
}

/// Every problem found, one line each.
#[derive(Debug)]
pub(super) struct Problems(pub(super) Vec<String>);

impl fmt::Display for Problems {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "description: {} problem{}; fix them all, then run again (`make describe-check` checks without shipping):",
            self.0.len(),
            if self.0.len() == 1 { "" } else { "s" }
        )?;
        for (index, problem) in self.0.iter().enumerate() {
            write!(out, "\n  {}. {problem}", index + 1)?;
        }
        Ok(())
    }
}

impl std::error::Error for Problems {}

/// `make ship`: every description problem first (before the lock wait, the
/// gates and the review), then the lock, then the ship.
pub(super) fn publish(root: &Path, options: &Options) -> super::Result<()> {
    if let Some(body) = &options.body_file {
        super::stack::refuse_base(&options.base, &git::branch(root)?)?;
        let situation = github::situation(root, &options.base).ok();
        early(root, options, body, situation.as_ref(), true)?;
    }
    let _lock = super::lock::acquire(root, "make ship")?;
    super::ship(root, options).map(|_| ())
}

/// The check `make ship` runs up front, with the base, parent and scope of
/// the ship itself (`situation` and its scope live until it returns). A failed `gh` lookup
/// leaves the title and a local parent to the preflight, which stops on it.
pub(super) fn early(
    root: &Path,
    options: &Options,
    body: &Path,
    situation: Option<&github::Situation>,
    fetch: bool,
) -> Result<Checked, Problems> {
    run(
        root,
        &Request {
            body,
            title: options.title.as_deref(),
            title_required: situation.is_some_and(github::Situation::title_required),
            video: options.video.as_deref(),
            base: &options.base,
            fetch,
        },
    )
}

/// `make describe-check`: the description check alone; read-only, no lock.
pub(super) fn command(root: &Path) -> super::Result<()> {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let body = env("SHIP_BODY").map(PathBuf::from).ok_or(
        "SHIP_BODY is required: `make describe-check SHIP_BODY=<file> [SHIP_TITLE=…] [SHIP_VIDEO=<file>]`",
    )?;
    let options = Options {
        title: env("SHIP_TITLE"),
        video: env("SHIP_VIDEO").map(PathBuf::from),
        base: env("SHIP_BASE").unwrap_or_else(|| "dev".into()),
        ..Options::default()
    };
    super::stack::refuse_base(&options.base, &git::branch(root)?)?;
    let situation = github::situation(root, &options.base);
    let checked = early(root, &options, &body, situation.as_ref().ok(), false)?;
    println!("{}", summary(&checked.decision));
    if situation.is_err() && options.title.is_none() {
        println!("title: not checked (gh unavailable)");
    }
    Ok(())
}

/// The one line `make describe-check` prints for a valid description.
pub(super) fn summary(decision: &Decision) -> String {
    format!(
        "description: OK (level {}, floor {})",
        decision.chosen.name(),
        decision.floor.name()
    )
}

pub(super) fn run(root: &Path, request: &Request) -> Result<Checked, Problems> {
    let text = match fs::read_to_string(request.body) {
        Ok(text) => text,
        Err(error) => {
            return Err(Problems(vec![format!(
                "SHIP_BODY {} cannot be read ({error}); point it at the description file",
                request.body.display()
            )]));
        }
    };
    let levels = match Levels::load(root) {
        Ok(levels) => levels,
        Err(error) => return Err(Problems(vec![error])),
    };
    let found = Template::inspect(&text, &levels);
    let mut problems = found.problems;
    let mut video = request.video.map(|path| {
        github::probe_video(root, path)
            .map_err(|error| problems.push(error.to_string()))
            .ok()
    });
    match request.title {
        None if request.title_required => {
            problems.push("a new pull request needs SHIP_TITLE: pass a short title".to_owned())
        }
        Some(title) if title.trim().is_empty() => {
            problems.push("SHIP_TITLE is empty: pass a short title".to_owned());
        }
        _ => {}
    }
    let mut decision = None;
    if let Some(level) = found.level {
        match Change::collect(root, &levels, request.base, request.fetch) {
            Ok(change) => {
                let (level_problems, floor) = level_problems(level, &levels, &change, video.take());
                problems.extend(level_problems);
                decision = Some(Decision {
                    chosen: level,
                    floor: floor.level,
                    reason: floor.reason,
                    metrics: levels.spec(level).metrics,
                });
            }
            Err(error) => problems.push(format!(
                "the level cannot be checked against the floor: {error}"
            )),
        }
    }
    match decision {
        Some(decision) if problems.is_empty() => Ok(Checked { text, decision }),
        _ => Err(Problems(problems)),
    }
}
