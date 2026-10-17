//! `make compare`: the same bookmarked camera on two builds of the real app,
//! as screenshots, a changed-pixel count of the scene and a montage
//! (docs/showcase.md, "Compare"). A viewer, not a gate: it exits 0 whatever
//! the differences and fails only when a view could not be captured.
pub(super) mod bookmarks;
pub(crate) mod build;
pub(super) mod capture;
pub(crate) mod config;
mod pixels;
pub(super) mod report;
pub(crate) mod server;
pub(super) mod settle;
mod side;
#[cfg(test)]
mod tests;
mod view;

pub(crate) use bookmarks::Bookmarks;
use capture::Shot;
pub(crate) use config::DEFAULT_BOOKMARKS;
use config::{Config, Source};
use report::{Report, Status};
use side::capture_side;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use view::compare_view;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn read_bounded(path: &Path, limit: u64) -> Result<String> {
    let length = fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if length > limit {
        return Err(format!("{}: {length} bytes, at most {limit}", path.display()).into());
    }
    Ok(fs::read_to_string(path)?)
}

fn git_line(root: &Path, args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// One side, resolved before anything is started.
struct Side {
    label: &'static str,
    source: Source,
    packages: PathBuf,
    hashes: BTreeMap<String, String>,
}

/// What every side of one run shares.
#[derive(Clone, Copy)]
struct Run<'a> {
    root: &'a Path,
    /// The commit `before` is built from.
    base: &'a str,
    /// The asset pack both servers run with.
    pack: &'a str,
    work: &'a Path,
    config: &'a Config,
    started: Instant,
}

pub(crate) fn execute(root: &Path) -> Result<()> {
    let config = Config::from_env(root, &|name| std::env::var(name).ok())?;
    let marks = Bookmarks::parse(&read_bounded(&config.bookmarks, config::BOOKMARKS_LIMIT)?)?;
    let views = marks.select(config.views.as_deref())?;
    let wanted: Vec<&str> = {
        let mut maps: Vec<&str> = views.iter().map(|b| b.map.as_str()).collect();
        maps.sort_unstable();
        maps.dedup();
        maps
    };
    let branch = git_line(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let head = git_line(root, &["rev-parse", "HEAD"]);
    let dirty = !git_line(root, &["status", "--porcelain"]).is_empty();
    let out = match &config.out {
        Some(path) => config::output_directory(root, path)?,
        None => root
            .join(".cache/compare")
            .join(config::branch_directory(&branch)),
    };
    let pack = config::asset_pack(root, config.asset_pack.as_deref())?;
    let base = match config.before {
        Source::Build => build::base_revision(root, config.base.as_deref())?,
        Source::Url(_) => String::new(),
    };
    let side = |label, source, packages: &PathBuf| -> Result<Side> {
        let hashes = server::resolve_maps(packages, &marks.maps, &wanted)?;
        Ok(Side {
            label,
            source,
            packages: packages.clone(),
            hashes,
        })
    };
    let sides = [
        side("before", config.before.clone(), &config.before_packages)?,
        side("after", config.after.clone(), &config.after_packages)?,
    ];
    fs::create_dir_all(&out)?;
    let scratch = root.join(".cache/compare/work");
    fs::create_dir_all(&scratch)?;
    let work = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&scratch)?;
    let started = Instant::now();
    eprintln!(
        "compare: {} views, before {}, after {}",
        views.len(),
        describe(&sides[0], &base),
        describe(&sides[1], &head)
    );
    let mut results = Vec::new();
    let run = Run {
        root,
        base: &base,
        pack: &pack,
        work: work.path(),
        config: &config,
        started,
    };
    for s in &sides {
        results.push(capture_side(&run, s, &views)?);
    }
    let (after_info, after_shots) = results.pop().ok_or("no after side")?;
    let (before_info, before_shots) = results.pop().ok_or("no before side")?;
    let mut reports = Vec::new();
    let mut rows = Vec::new();
    for mark in &views {
        let (view, row) = compare_view(
            mark,
            before_shots.get(&mark.name),
            after_shots.get(&mark.name),
            &out,
        )?;
        rows.extend(row);
        reports.push(view);
    }
    let (montage, shrink) = if rows.is_empty() {
        (None, 1)
    } else {
        let (sheet, shrink) = pixels::stack(&rows);
        fs::write(out.join("montage.png"), sheet.encode()?)?;
        (Some("montage.png".to_owned()), shrink)
    };
    let report = Report {
        branch,
        head: if dirty { format!("{head}-dirty") } else { head },
        base,
        viewport: capture::VIEWPORT,
        before: before_info,
        after: after_info,
        seconds: started.elapsed().as_secs(),
        views: reports,
        montage,
        montage_shrink: shrink,
    };
    fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", report::table(&report.views));
    println!(
        "compare: {} views in {}s, written to {}",
        report.views.len(),
        report.seconds,
        out.display()
    );
    let failed: Vec<&str> = report
        .views
        .iter()
        .filter(|v| matches!(v.status, Status::Failed))
        .map(|v| v.name.as_str())
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} view(s) could not be captured: {}",
            failed.len(),
            failed.join(", ")
        )
        .into())
    }
}

fn describe(side: &Side, revision: &str) -> String {
    match &side.source {
        Source::Build => format!("build of {}", revision.get(..12).unwrap_or(revision)),
        Source::Url(url) => url.clone(),
    }
}
