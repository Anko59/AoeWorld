//! Runs `capture.mjs` for one side and one renderer in the pinned browser
//! image: host network (the app is on this machine's loopback), a private work
//! directory, the container removed on every return path.
use super::{
    bookmarks::{Bookmark, Renderer},
    report::Capture,
};
use crate::process;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SCRIPT: &str = include_str!("capture.mjs");
pub(crate) const VIEWPORT: [u32; 2] = [1280, 720];
/// Polls a settled frame must stay unchanged for, and how often to poll.
const SETTLE_POLLS: u32 = 5;
const POLL_MS: u32 = 400;
/// Per view, on top of its settle timeout: opening a map, placing the camera.
const PLACEMENT: Duration = Duration::from_secs(60);
const STARTUP: Duration = Duration::from_secs(180);

/// One view of one side as captured: the screenshot, if any, and what the
/// page reported.
pub(crate) struct Shot {
    pub(crate) png: Option<Vec<u8>>,
    pub(crate) capture: Capture,
}

#[derive(Serialize)]
struct PlanView<'a> {
    name: &'a str,
    map_hash: &'a str,
    x: f64,
    y: f64,
    zoom: f64,
}

#[derive(Serialize)]
struct Plan<'a> {
    url: &'a str,
    renderer: &'static str,
    viewport: PlanViewport,
    settle_polls: u32,
    poll_ms: u32,
    view_timeout_ms: u64,
    views: Vec<PlanView<'a>>,
}

#[derive(Serialize)]
struct PlanViewport {
    width: u32,
    height: u32,
}

struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub(crate) struct Run<'a> {
    pub(crate) work: &'a Path,
    pub(crate) label: &'a str,
    pub(crate) url: &'a str,
    pub(crate) modules: &'a Path,
    pub(crate) image: &'a str,
    pub(crate) view_timeout: Duration,
    /// What is left of the whole run's budget.
    pub(crate) remaining: Duration,
}

/// `views` all use `renderer`; each is paired with its map's hash on this side.
pub(crate) fn run(
    run: &Run<'_>,
    renderer: Renderer,
    views: &[(&Bookmark, &str)],
) -> Result<BTreeMap<String, Shot>> {
    let dir = run.work.join(format!("{}-{}", run.label, renderer.name()));
    fs::create_dir(&dir)?;
    let plan = Plan {
        url: run.url,
        renderer: renderer.name(),
        viewport: PlanViewport {
            width: VIEWPORT[0],
            height: VIEWPORT[1],
        },
        settle_polls: SETTLE_POLLS,
        poll_ms: POLL_MS,
        view_timeout_ms: u64::try_from(run.view_timeout.as_millis())?,
        views: views
            .iter()
            .map(|(b, hash)| PlanView {
                name: &b.name,
                map_hash: hash,
                x: b.x,
                y: b.y,
                zoom: b.zoom,
            })
            .collect(),
    };
    fs::write(dir.join("plan.json"), serde_json::to_vec(&plan)?)?;
    fs::write(dir.join("capture.mjs"), SCRIPT)?;
    let wanted = STARTUP + (run.view_timeout + PLACEMENT) * u32::try_from(views.len())?;
    if run.remaining < Duration::from_secs(30) {
        return Err("the run's time budget (COMPARE_BUDGET_MINUTES) is used up".into());
    }
    let deadline = wanted.min(run.remaining);
    let name = format!(
        "aoeworld-compare-{}-{}-{}",
        std::process::id(),
        run.label,
        renderer.name()
    );
    let _cleanup = Container(name.clone());
    let user = format!("{}:{}", nix::unistd::getuid(), nix::unistd::getgid());
    let dir_text = dir.display().to_string();
    let args = [
        "run",
        "--rm",
        "--init",
        "--network",
        "host",
        "--ipc",
        "host",
        "--user",
        &user,
        "--name",
        &name,
        "-e",
        "HOME=/tmp",
        "-w",
        &dir_text,
        "-v",
        &format!("{dir_text}:{dir_text}"),
        "-v",
        &format!("{}:{dir_text}/node_modules:ro", run.modules.display()),
        run.image,
        "node",
        "capture.mjs",
    ];
    process::run("docker", &args, deadline).map_err(|e| {
        format!(
            "the {} browser run ({}) failed: {e}",
            run.label,
            renderer.name()
        )
    })?;
    let mut shots = BTreeMap::new();
    for (bookmark, _) in views {
        let capture: Capture = fs::read(dir.join(format!("{}.json", bookmark.name)))
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
            .unwrap_or_else(|e| Capture {
                error: Some(format!("no capture result: {e}")),
                ..Capture::default()
            });
        let png = fs::read(dir.join(format!("{}.png", bookmark.name)))
            .ok()
            .filter(|_| capture.error.is_none());
        shots.insert(bookmark.name.clone(), Shot { png, capture });
    }
    Ok(shots)
}
