//! Runs `capture.mjs` for one side, one map and one renderer in the pinned
//! browser image: host network (the app is on this machine's loopback), a
//! private work directory, the container removed on every return path. The
//! page is a spectator opening the camera fragment of each view; no controller.
use super::{
    bookmarks::{Bookmark, Renderer},
    report::{Camera, Capture},
    settle::{self, Verdict},
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
/// How often the page polls `#scene` and `#minimap` while a view settles.
const POLL_MS: u32 = 250;
/// Per view, on top of its settle timeout: placing the camera, screenshots.
const PLACEMENT: Duration = Duration::from_secs(30);
/// The page load (WASM, assets, the opened package) before the first view.
const STARTUP: Duration = Duration::from_secs(180);

/// One view of one side as captured: the screenshot, if any, and what the
/// page reported. A view the checks refuse has no screenshot and says why.
pub(crate) struct Shot {
    pub(crate) png: Option<Vec<u8>>,
    pub(crate) capture: Capture,
}

/// The camera fragment that places a bookmark: `#x=…&y=…&zoom=…&renderer=…`,
/// as `web/navigation.js` reads it. Rust's float text is the shortest that
/// round-trips, so the page gets the bookmark's exact numbers.
pub(crate) fn fragment(mark: &Bookmark) -> String {
    format!(
        "x={}&y={}&zoom={}&renderer={}",
        mark.x,
        mark.y,
        mark.zoom,
        mark.renderer.name()
    )
}

/// The first reason a side's capture of `mark` cannot be compared, if any:
/// the page's own error, a view that did not settle or did not fail cleanly,
/// the wrong map, the wrong renderer or a camera that is not the bookmark's.
pub(crate) fn refusal(mark: &Bookmark, map_hash: &str, capture: &Capture) -> Option<String> {
    if let Some(error) = &capture.error {
        return Some(error.clone());
    }
    match settle::decide(&capture.history) {
        Verdict::Settled { .. } => {}
        Verdict::Failed { failed } => return Some(format!("{failed} chunks failed to load")),
        Verdict::Unsettled { pending, failed } => {
            return Some(format!("not settled (pending {pending}, failed {failed})"));
        }
        Verdict::NoSamples => return Some("the page never reported the scene".into()),
    }
    if capture.map_hash.as_deref() != Some(map_hash) {
        return Some(format!(
            "the page has map {} open, not {map_hash}: start the server with AOE_OPEN_MAP_PACKAGE",
            capture.map_hash.as_deref().unwrap_or("none")
        ));
    }
    if capture.renderer.as_deref() != Some(mark.renderer.name()) {
        return Some(format!(
            "renderer: the bookmark asks for {}, the page ran {}",
            mark.renderer.name(),
            capture.renderer.as_deref().unwrap_or("none")
        ));
    }
    let wanted = Camera {
        x: mark.x,
        y: mark.y,
        zoom: mark.zoom,
    };
    match capture.camera {
        Some(camera) if camera.same_view(&wanted) => None,
        Some(camera) => Some(format!(
            "camera at {:.2},{:.2} z{:.3}, the bookmark asks {:.2},{:.2} z{:.3}",
            camera.x, camera.y, camera.zoom, wanted.x, wanted.y, wanted.zoom
        )),
        None => Some("no camera read back".into()),
    }
}

#[derive(Serialize)]
struct PlanView<'a> {
    name: &'a str,
    fragment: String,
    x: f64,
    y: f64,
    zoom: f64,
}

#[derive(Serialize)]
struct Plan<'a> {
    url: &'a str,
    map_hash: &'a str,
    renderer: &'static str,
    viewport: [u32; 2],
    poll_ms: u32,
    view_timeout_ms: u64,
    views: Vec<PlanView<'a>>,
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

/// `views` are bookmarks of `renderer` on the map whose package is `map_hash`
/// and which the server at `run.url` has open.
pub(crate) fn run(
    run: &Run<'_>,
    renderer: Renderer,
    map_hash: &str,
    views: &[&Bookmark],
) -> Result<BTreeMap<String, Shot>> {
    let short = map_hash.get(..12).unwrap_or(map_hash);
    let dir = run
        .work
        .join(format!("{}-{short}-{}", run.label, renderer.name()));
    fs::create_dir(&dir)?;
    let plan = Plan {
        url: run.url,
        map_hash,
        renderer: renderer.name(),
        viewport: VIEWPORT,
        poll_ms: POLL_MS,
        view_timeout_ms: u64::try_from(run.view_timeout.as_millis())?,
        views: views
            .iter()
            .map(|b| PlanView {
                name: &b.name,
                fragment: fragment(b),
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
        "aoeworld-compare-{}-{}-{short}-{}",
        std::process::id(),
        run.label,
        renderer.name()
    );
    let _cleanup = Container(name.clone());
    let user = format!("{}:{}", nix::unistd::getuid(), nix::unistd::getgid());
    let dir_text = dir.display().to_string();
    let mut args = vec![
        "run".to_owned(),
        "--rm".into(),
        "--init".into(),
        "--network".into(),
        "host".into(),
        "--ipc".into(),
        "host".into(),
        "--user".into(),
        user,
        "--name".into(),
        name,
        "-e".into(),
        "HOME=/tmp".into(),
        "-w".into(),
        dir_text.clone(),
        "-v".into(),
        format!("{dir_text}:{dir_text}"),
        "-v".into(),
        format!("{}:{dir_text}/node_modules:ro", run.modules.display()),
    ];
    args.extend([run.image.to_owned(), "node".into(), "capture.mjs".into()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    process::run("docker", &args, deadline).map_err(|e| {
        format!(
            "the {} browser run ({}, {short}) failed: {e}",
            run.label,
            renderer.name()
        )
    })?;
    let mut shots = BTreeMap::new();
    for mark in views {
        let capture: Capture = fs::read(dir.join(format!("{}.json", mark.name)))
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
            .unwrap_or_else(|e| Capture {
                error: Some(format!("no capture result: {e}")),
                ..Capture::default()
            });
        let (png, capture) = match refusal(mark, map_hash, &capture) {
            Some(reason) => (
                None,
                Capture {
                    error: Some(reason),
                    ..capture
                },
            ),
            None => (
                fs::read(dir.join(format!("{}.png", mark.name))).ok(),
                capture,
            ),
        };
        shots.insert(mark.name.clone(), Shot { png, capture });
    }
    Ok(shots)
}
