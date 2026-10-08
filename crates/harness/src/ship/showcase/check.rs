//! Validation used before Make builds the showcase images.
use super::Storyboard;
use std::{os::unix::ffi::OsStrExt, path::Path};

pub(crate) use super::media::check;

pub(super) fn showcase_level(root: &Path) -> Result<super::super::describe::Level, String> {
    let selected = std::env::var("SHOWCASE_LEVEL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("SHIP_LEVEL")
                .ok()
                .filter(|value| !value.is_empty())
        });
    let configured = if selected.is_none() {
        std::fs::read_to_string(root.join(".github/pull_request_template.md")).ok()
    } else {
        None
    };
    let name = selected.or_else(|| {
        configured.and_then(|text| {
            text.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("<!-- level:")?
                    .strip_suffix("-->")
                    .map(str::trim)
                    .map(str::to_owned)
            })
        })
    });
    let name = name.ok_or_else(|| {
        "SHOWCASE_LEVEL is required when SHIP_LEVEL and the pull request level config are unset; use low, medium, high or max".to_owned()
    })?;
    super::super::describe::Level::parse(&name)
        .ok_or_else(|| format!("SHOWCASE_LEVEL={name}: use low, medium, high or max"))
}

pub(crate) fn validate_manifest_root(root: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    if root
        .as_os_str()
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\n' | b'\r'))
    {
        return Err("showcase manifest paths cannot contain a newline or carriage return".into());
    }
    Ok(())
}

pub(crate) fn check_voice_requirement(
    level: super::super::describe::Level,
    board: &Storyboard,
) -> Result<(), String> {
    if level.voiced() && board.scenes.iter().all(|scene| scene.narration().is_none()) {
        return Err(format!(
            "{} showcases need at least one narrated scene (SHOWCASE_LEVEL)",
            level.name()
        ));
    }
    Ok(())
}
