//! Compare bookmarks and verification, applied before speech or image builds.
//! Apps stay pre-started: each build's map directory describes the package its
//! server opened, and its origin is reused by every scene and later take.
use super::{browser::Plan, origins::Build};
use crate::ship::compare::{
    bookmarks::{Bookmark, Bookmarks},
    capture, config,
    report::Capture,
    server,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Serialize)]
pub(crate) struct View {
    #[serde(flatten)]
    mark: Bookmark,
    map_hash: String,
}

fn setting(root: &Path, name: &str, default: &str) -> PathBuf {
    root.join(
        std::env::var(name)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default.into()),
    )
}

pub(super) fn apply(root: &Path, plan: &mut Plan<'_>) -> Result<()> {
    if !plan.scenes.iter().any(|s| {
        matches!(
            s.scene,
            super::Scene::Browser {
                bookmark: Some(_),
                ..
            }
        )
    }) {
        return Ok(());
    }
    let file = setting(root, "COMPARE_BOOKMARKS", config::DEFAULT_BOOKMARKS);
    let marks = Bookmarks::parse(&super::media::read_storyboard(&file)?)?;
    let mut selected = Vec::with_capacity(plan.scenes.len());
    for scene in &plan.scenes {
        let mark = match scene.scene {
            super::Scene::Browser {
                bookmark: Some(name),
                ..
            } => Some(
                marks
                    .bookmarks
                    .iter()
                    .find(|m| m.name == *name)
                    .ok_or_else(|| format!("showcase: no bookmark `{name}` in {}", file.display()))?
                    .clone(),
            ),
            _ => None,
        };
        selected.push(mark);
    }
    for (build, variable) in [
        (Build::Before, "COMPARE_BEFORE_PACKAGES"),
        (Build::After, "COMPARE_AFTER_PACKAGES"),
    ] {
        let mut wanted: Vec<&str> = plan
            .scenes
            .iter()
            .zip(&selected)
            .filter_map(|(scene, mark)| {
                matches!(scene.scene, super::Scene::Browser { build: b, .. } if *b == build)
                    .then_some(mark.as_ref())
                    .flatten()
                    .map(|m| m.map.as_str())
            })
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        if wanted.is_empty() {
            continue;
        }
        let packages = setting(root, variable, "local-assets/map-packages");
        let hashes = server::resolve_maps(&packages, &marks.maps, &wanted)?;
        for (scene, mark) in plan.scenes.iter_mut().zip(&selected) {
            if !matches!(scene.scene, super::Scene::Browser { build: b, .. } if *b == build) {
                continue;
            }
            if let Some(mark) = mark {
                let mut url =
                    url::Url::parse(scene.url.as_deref().ok_or("bookmark has no app origin")?)?;
                url.set_fragment(Some(&capture::fragment(mark)));
                scene.url = Some(url.into());
                scene.view = Some(View {
                    mark: mark.clone(),
                    map_hash: hashes[&mark.map].clone(),
                });
                if mark.renderer == crate::ship::compare::bookmarks::Renderer::Webgpu {
                    plan.renderer = super::Renderer::Webgpu;
                }
            }
        }
    }
    Ok(())
}

/// Replay the exact bookmark observation with compare's Rust verdict. No
/// output is published when hooks, map, renderer or camera disagree.
pub(super) fn verify(work: &Path, plan: &Plan<'_>) -> Result<()> {
    if plan.scenes.iter().all(|s| s.view.is_none()) {
        return Ok(());
    }
    let evidence: Vec<Option<Capture>> =
        serde_json::from_slice(&fs::read(work.join("views.json"))?)?;
    if evidence.len() != plan.scenes.len() {
        return Err("showcase: incomplete view evidence".into());
    }
    for (index, (scene, observed)) in plan.scenes.iter().zip(evidence).enumerate() {
        if let Some(view) = &scene.view {
            let observed = observed.ok_or("showcase: no bookmark observation")?;
            if let Some(reason) = capture::refusal(&view.mark, &view.map_hash, &observed) {
                return Err(format!("showcase: scene {}: {reason}", index + 1).into());
            }
        }
    }
    Ok(())
}
