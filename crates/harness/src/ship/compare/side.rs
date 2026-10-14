//! One side of a comparison: its build (or running app), one server per map it
//! shows, and one spectator browser run per renderer of each map. A server
//! opens its map at start (AOE_OPEN_MAP_PACKAGE), so no session needs a lease.
use super::{
    Run, Side,
    bookmarks::{Bookmark, Renderer},
    build, capture,
    config::Source,
    report::SideInfo,
    server,
};
use std::{collections::BTreeMap, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Capture every view on one side.
pub(super) fn capture_side(
    run: &Run<'_>,
    side: &Side,
    views: &[&Bookmark],
) -> Result<(SideInfo, BTreeMap<String, capture::Shot>)> {
    let Run {
        root,
        base,
        pack,
        work,
        config,
        started,
    } = *run;
    let mut unique: Vec<&str> = Vec::new();
    for mark in views {
        if !unique.contains(&mark.map.as_str()) {
            unique.push(mark.map.as_str());
        }
    }
    if matches!(side.source, Source::Url(_)) && unique.len() > 1 {
        return Err(format!(
            "{} is a running app, which has one map open: select views of one map",
            side.label
        )
        .into());
    }
    let (tree, revision) = match &side.source {
        Source::Url(_) => (None, None),
        Source::Build if side.label == "before" => {
            (Some(build::base_tree(root, base)?), Some(base.to_owned()))
        }
        Source::Build => (
            Some(build::working_tree(root)?),
            Some(super::git_line(root, &["rev-parse", "HEAD"])),
        ),
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
    let mut urls = Vec::new();
    for map in &unique {
        let hash = side.hashes[*map].as_str();
        let (mut owned, url) = match (&side.source, &tree) {
            (Source::Url(url), _) => (None, url.clone()),
            (Source::Build, Some(tree)) => {
                let log = work.join(format!("{}-{map}-server.log", side.label));
                let server = server::Server::start(
                    &tree.binary,
                    &tree.root,
                    &root.join("local-assets"),
                    pack,
                    &side.packages,
                    hash,
                    &log,
                )
                .map_err(|e| crate::e2e::stale::annotate(e, &side.packages))?;
                let url = server.url.clone();
                (Some(server), url)
            }
            (Source::Build, None) => return Err("the build was not made".into()),
        };
        urls.push(url.clone());
        for renderer in [Renderer::Webgpu, Renderer::Webgl2] {
            let group: Vec<&Bookmark> = views
                .iter()
                .copied()
                .filter(|b| b.map == *map && b.renderer == renderer)
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
            shots.extend(capture::run(&browser, renderer, hash, &group).map_err(|e| {
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
    }
    let info = SideInfo {
        source: match &side.source {
            Source::Build => "build".into(),
            Source::Url(u) => u.clone(),
        },
        revision,
        url: urls.join(", "),
        packages: side.packages.display().to_string(),
        map_hashes: side.hashes.clone(),
        controller_lease: "none: spectator sessions, no controller on either side".to_owned(),
    };
    Ok((info, shots))
}
