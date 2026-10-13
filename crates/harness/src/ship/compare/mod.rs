//! `make compare`: the same bookmarked camera on two builds of the real app,
//! as screenshots, a changed-pixel count of the scene and a montage
//! (docs/showcase.md, "Compare"). A viewer, not a gate: it exits 0 whatever
//! the differences and fails only when a view could not be captured.
mod bookmarks;
mod build;
mod capture;
mod config;
mod pixels;
mod report;
mod server;
#[cfg(test)]
mod tests;

use bookmarks::{Bookmark, Bookmarks, Renderer};
use capture::Shot;
use config::{Config, Source};
use pixels::Image;
use report::{Observed, Report, Requested, SideInfo, Status, ViewReport};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

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

/// Capture every view on one side: serve the build (or use the running app),
/// then one browser run per renderer in use.
fn capture_side(
    run: &Run<'_>,
    side: &Side,
    views: &[&Bookmark],
) -> Result<(SideInfo, BTreeMap<String, Shot>)> {
    let Run {
        root,
        base,
        pack,
        work,
        config,
        started,
    } = *run;
    let (mut owned, url, revision) = match &side.source {
        Source::Url(url) => (None, url.clone(), None),
        Source::Build => {
            let (tree, revision) = if side.label == "before" {
                (build::base_tree(root, base)?, base.to_owned())
            } else {
                (
                    build::working_tree(root)?,
                    git_line(root, &["rev-parse", "HEAD"]),
                )
            };
            let log = work.join(format!("{}-server.log", side.label));
            let local_assets = root.join("local-assets");
            let server = server::Server::start(
                &tree.binary,
                &tree.root,
                &local_assets,
                pack,
                &side.packages,
                &log,
            )
            .map_err(|e| crate::e2e::stale::annotate(e, &side.packages))?;
            let url = server.url.clone();
            (Some(server), url, Some(revision))
        }
    };
    let modules = root.join("browser/node_modules");
    if !modules.join("playwright").is_dir() {
        return Err(format!(
            "{} has no Playwright: run `make browser-deps`",
            modules.display()
        )
        .into());
    }
    let image = std::env::var("BROWSER_IMAGE")
        .map_err(|_| "BROWSER_IMAGE is unset: run through `make compare`")?;
    let mut shots = BTreeMap::new();
    for renderer in [Renderer::Webgpu, Renderer::Webgl2] {
        let group: Vec<(&Bookmark, &str)> = views
            .iter()
            .filter(|b| b.renderer == renderer)
            .map(|b| (*b, side.hashes[&b.map].as_str()))
            .collect();
        if group.is_empty() {
            continue;
        }
        let browser = capture::Run {
            work,
            label: side.label,
            url: &url,
            modules: &modules,
            image: &image,
            view_timeout: Duration::from_secs(config.view_seconds),
            remaining: Duration::from_secs(config.budget_minutes * 60)
                .saturating_sub(started.elapsed()),
        };
        shots.extend(capture::run(&browser, renderer, &group).map_err(|e| {
            let tail = owned.as_ref().map(|s| s.log_tail()).unwrap_or_default();
            format!("{e}\nserver log tail:\n{tail}")
        })?);
    }
    if let Some(server) = owned.as_mut()
        && !server.alive()
    {
        return Err(format!(
            "the {} server died during the run:\n{}",
            side.label,
            server.log_tail()
        )
        .into());
    }
    let lease = match &side.source {
        Source::Build => "one session, the controller of this run's own server".to_owned(),
        Source::Url(_) => "one session on a running server: it took that server's controller lease when free, else the map could not be opened".to_owned(),
    };
    let info = SideInfo {
        source: match &side.source {
            Source::Build => "build".into(),
            Source::Url(u) => u.clone(),
        },
        revision,
        url,
        packages: side.packages.display().to_string(),
        map_hashes: side.hashes.clone(),
        controller_lease: lease,
    };
    Ok((info, shots))
}

fn observed(shot: &Shot) -> Option<Observed> {
    let c = &shot.capture;
    Some(Observed {
        camera: c.camera?,
        chunks_loaded: c.chunks.unwrap_or(0),
        settle_ms: c.settle_ms.unwrap_or(0),
        reported_renderer: c.renderer.clone(),
        page_errors: c.page_errors.clone(),
    })
}

/// A view's two shots as a report row, the files it writes and the montage row.
fn compare_view(
    mark: &Bookmark,
    before: Option<&Shot>,
    after: Option<&Shot>,
    out: &Path,
) -> Result<(ViewReport, Option<Image>)> {
    let mut view = ViewReport {
        name: mark.name.clone(),
        map: mark.map.clone(),
        renderer: mark.renderer,
        requested: Requested {
            x: mark.x,
            y: mark.y,
            zoom: mark.zoom,
        },
        status: Status::Failed,
        before: before.and_then(observed),
        after: after.and_then(observed),
        scene: None,
        masks: Vec::new(),
        compared_pixels: None,
        changed_pixels: None,
        changed_percent: None,
        masked_changed_pixels: None,
        only_masked_differ: false,
        error: None,
        files: Vec::new(),
    };
    let problems: Vec<String> = [("before", before), ("after", after)]
        .iter()
        .filter_map(|(side, shot)| match shot {
            Some(Shot {
                png: Some(_),
                capture,
            }) if capture.camera.is_some() && capture.scene.is_some() => None,
            Some(Shot { capture, .. }) => Some(format!(
                "{side}: {}",
                capture.error.as_deref().unwrap_or("no screenshot")
            )),
            None => Some(format!("{side}: not captured")),
        })
        .collect();
    let (Some(before), Some(after), true) = (before, after, problems.is_empty()) else {
        view.error = Some(problems.join("; "));
        return Ok((view, None));
    };
    let (Some(bpng), Some(apng)) = (before.png.as_deref(), after.png.as_deref()) else {
        view.error = Some("no screenshot".into());
        return Ok((view, None));
    };
    let mut put = |suffix: &str, bytes: &[u8]| -> Result<()> {
        let file = format!("{}.{suffix}.png", mark.name);
        fs::write(out.join(&file), bytes)?;
        view.files.push(file);
        Ok(())
    };
    put("before", bpng)?;
    put("after", apng)?;
    let (bimg, aimg) = (Image::decode(bpng)?, Image::decode(apng)?);
    let scene = after.capture.scene.unwrap_or_default();
    let mut masks = after.capture.masks.clone();
    for m in &before.capture.masks {
        if !masks.contains(m) {
            masks.push(*m);
        }
    }
    let (Some(bcam), Some(acam)) = (before.capture.camera, after.capture.camera) else {
        return Err("a captured view has no camera".into());
    };
    view.scene = Some(scene);
    view.masks = masks.clone();
    let crop = |i: &Image| i.crop(scene);
    if !bcam.same_view(&acam) {
        view.status = Status::CameraMismatch;
        view.error = Some(format!(
            "before ended at {:.2},{:.2} z{:.3}, after at {:.2},{:.2} z{:.3}",
            bcam.x, bcam.y, bcam.zoom, acam.x, acam.y, acam.zoom
        ));
        let row = pixels::row(
            &mark.name,
            &[
                ("BEFORE", &crop(&bimg)),
                ("AFTER (CAMERA MISMATCH)", &crop(&aimg)),
            ],
        );
        fs::write(
            out.join(format!("{}.montage.png", mark.name)),
            row.encode()?,
        )?;
        view.files.push(format!("{}.montage.png", mark.name));
        return Ok((view, Some(row)));
    }
    let diff = pixels::diff(&bimg, &aimg, scene, &masks)?;
    view.compared_pixels = Some(diff.compared);
    view.changed_pixels = Some(diff.changed);
    view.changed_percent = Some(diff.percent());
    view.masked_changed_pixels = Some(diff.masked_changed);
    view.only_masked_differ = diff.changed == 0 && diff.masked_changed > 0;
    view.status = if diff.changed == 0 {
        Status::Identical
    } else {
        Status::Changed
    };
    put("diff", &diff.image.encode()?)?;
    let caption = if diff.changed == 0 {
        "DIFF: IDENTICAL".to_owned()
    } else {
        format!("DIFF: {} PX ({:.3}%)", diff.changed, diff.percent())
    };
    let row = pixels::row(
        &mark.name,
        &[
            ("BEFORE", &crop(&bimg)),
            ("AFTER", &crop(&aimg)),
            (&caption, &crop(&diff.image)),
        ],
    );
    fs::write(
        out.join(format!("{}.montage.png", mark.name)),
        row.encode()?,
    )?;
    view.files.push(format!("{}.montage.png", mark.name));
    Ok((view, Some(row)))
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
