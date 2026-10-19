//! `make dev-server`: one long-lived game server per machine, built from
//! `origin/dev` in a checkout the harness owns (`<git common dir>/aoe-dev-server`),
//! on a fixed port, with the start screen's preset maps prepared once per
//! map-code fingerprint and kept. `make compare` and `make showcase` use it as
//! their "before" side when it is healthy at the revision they expect, and say
//! why when they do not (docs/dev-server.md).
mod build;
mod ensure;
mod layout;
mod presets;
mod process;
mod revision;
mod state;
#[cfg(test)]
mod tests;
mod warm;

use ensure::{PLAY, Settings, short};
use layout::Layout;
use state::{State, now};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// `make dev-server`: make the play server current and report it.
pub(crate) fn start(root: &Path) -> Result<()> {
    let settings = Settings::current()?;
    let kept = ensure::ensure_play(root, &settings)?;
    println!(
        "dev-server: {} {}",
        if kept { "already current" } else { "started" },
        line(root, &settings, &Layout::resolve(root)?)?
    );
    Ok(())
}

/// `make dev-server-status`.
pub(crate) fn status(root: &Path) -> Result<()> {
    let settings = Settings::current()?;
    println!(
        "dev-server: {}",
        line(root, &settings, &Layout::resolve(root)?)?
    );
    Ok(())
}

/// `make dev-server-stop`: stop every server; built trees and prepared maps stay.
pub(crate) fn stop(root: &Path) -> Result<()> {
    let layout = Layout::resolve(root)?;
    let _held = ensure::lock(&layout)?;
    let mut state = State::read(&layout.state());
    for instance in std::mem::take(&mut state.instances) {
        instance.halt(&layout);
        println!(
            "dev-server: stopped {} on port {}",
            instance.name, instance.port
        );
    }
    state.write(&layout.state())?;
    Ok(())
}

/// The status line: revision, port, uptime, maps ready, footprint.
fn line(root: &Path, settings: &Settings, layout: &Layout) -> Result<String> {
    let state = State::read(&layout.state());
    let tip = revision::expected(root, settings.rev.as_deref()).ok();
    let Some(play) = state.find(PLAY) else {
        return Ok(format!(
            "stopped (nothing started; origin/dev is {})",
            tip.as_deref().map_or("unknown", short)
        ));
    };
    let health = process::health_build(play.port);
    let alive = play.alive(layout);
    let mut parts = Vec::new();
    match (&health, alive) {
        (Some(build), true) => parts.push(format!("running revision {}", short(build))),
        (_, true) => parts.push(format!("unhealthy revision {}", short(&play.revision))),
        _ => parts.push(format!("stopped (was revision {})", short(&play.revision))),
    }
    parts.push(format!("port {}", play.port));
    parts.push(format!(
        "uptime {}",
        duration(now().saturating_sub(play.started))
    ));
    if let (Some(_), Some(fingerprint)) = (
        health.as_ref(),
        revision::fingerprint(root, &play.revision).ok(),
    ) {
        let list = layout.presets(&fingerprint).join("presets.json");
        let wanted: Vec<serde_json::Value> = std::fs::read(&list)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let (ready, total) = presets::ready(play.port, &wanted);
        parts.push(format!("maps ready {ready}/{total}"));
    }
    if let Some(tip) = &tip
        && health.as_deref().is_some_and(|b| b != tip)
    {
        parts.push(format!(
            "STALE: dev is at {} (run `make dev-server`)",
            short(tip)
        ));
    }
    let views = state.instances.len().saturating_sub(1);
    if views > 0 {
        parts.push(format!("{views} compare view server(s)"));
    }
    parts.push(format!(
        "footprint {} MiB (trees {}, target {}, maps {})",
        build::size(&layout.home) >> 20,
        build::size(&layout.trees()) >> 20,
        build::size(&layout.target()) >> 20,
        build::size(&layout.all_presets()) >> 20
    ));
    Ok(parts.join(", "))
}

fn duration(seconds: u64) -> String {
    match seconds {
        0..=119 => format!("{seconds}s"),
        120..=7199 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, seconds % 3600 / 60),
    }
}

/// The map request the setup form submits for a preset (`finland`) at a side
/// in kilometres, or `None` for a preset the form does not have.
pub(crate) fn skirmish_request(preset: &str, kilometres: u64) -> Option<serde_json::Value> {
    presets::named_request(preset, kilometres)
}

/// Make every completed map of `wanted` on `port` ready to start: serve its
/// minimap raster (warm.rs). Returns what was warmed, each with the time its
/// first request took.
pub(crate) fn warm_maps(
    port: u16,
    wanted: &[serde_json::Value],
) -> std::result::Result<Vec<(String, std::time::Duration)>, String> {
    warm::maps(port, wanted, warm::BUDGET)
}

/// Showcase's `before` side: the play server's URL when it is healthy and at
/// the revision dev is at; otherwise why not.
pub(crate) fn play_url(root: &Path) -> std::result::Result<String, String> {
    let settings = Settings::current()?;
    let layout = Layout::resolve(root)?;
    let expected = revision::expected(root, settings.rev.as_deref())?;
    let state = State::read(&layout.state());
    let Some(play) = state.find(PLAY) else {
        return Err("the dev server is not running (`make dev-server` starts it)".into());
    };
    if !play.alive(&layout) {
        return Err("the dev server's process is gone (`make dev-server` restarts it)".into());
    }
    match process::health_build(play.port) {
        None => Err(format!(
            "the dev server on port {} does not answer /health",
            play.port
        )),
        Some(build) if build != expected => Err(format!(
            "the dev server serves {} and dev is at {} (`make dev-server` rebuilds it)",
            short(&build),
            short(&expected)
        )),
        Some(_) => Ok(format!("http://127.0.0.1:{}/", play.port)),
    }
}

/// Compare's `before` side: whether the shared build can stand in for the
/// merge base `base` (it can only when dev has not moved past it).
pub(crate) fn serves_base(root: &Path, base: &str) -> std::result::Result<(), String> {
    let settings = Settings::current()?;
    let expected = revision::expected(root, settings.rev.as_deref())?;
    if expected == base {
        Ok(())
    } else {
        Err(format!(
            "the merge base {} is not what the dev server tracks ({})",
            short(base),
            short(&expected)
        ))
    }
}

/// Compare's `before` view server for one map, built and started on first use
/// and shared afterwards.
pub(crate) fn view_url(
    root: &Path,
    base: &str,
    pack: &str,
    packages: &Path,
    open: &str,
) -> std::result::Result<String, String> {
    let settings = Settings::current()?;
    ensure::ensure_view(root, &settings, base, pack, packages, open).map_err(|e| e.to_string())
}
