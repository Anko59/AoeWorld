//! `make showcase`: the PR's showcase video from a storyboard
//! (docs/showcase.md). Scenes are title cards and scripted terminal
//! transcripts (tagged before/after). Playwright in the pinned browser image
//! records them in one take; narration is voiced
//! with Gemini Flash Lite TTS on OpenRouter and mixed in with ffmpeg from the
//! pinned ship-tools image. Each scene lasts at least as long as its voice.
mod check;
mod media;

pub(crate) use check::check;
pub(crate) use media::make;
#[cfg(test)]
pub(crate) use media::read_storyboard;
#[cfg(test)]
pub(crate) use media::{concat_entry, publish_temp_output, tts_command};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub(crate) const TTS_MODEL: &str = "google/gemini-3.8-flash-lite-tts";
/// OpenRouter's limit on one speech request.
pub(crate) const TTS_MAX_CHARS: usize = 3000;
/// Gemini TTS on OpenRouter answers raw PCM only: 16-bit mono at 24 kHz.
pub(crate) const PCM_RATE: u32 = 24_000;

/// Seconds of speech in `bytes` of that PCM.
pub(crate) fn pcm_seconds(bytes: u64) -> f64 {
    bytes as f64 / f64::from(PCM_RATE * 2)
}

/// The longest level (max) allows five minutes.
pub(crate) const MAX_SECONDS: u64 = 300;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Storyboard {
    pub(crate) title: String,
    /// A Gemini TTS voice: Kore, Puck, Charon, …
    #[serde(default = "default_voice")]
    pub(crate) voice: String,
    pub(crate) scenes: Vec<Scene>,
}

fn default_voice() -> String {
    "Kore".into()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Scene {
    Card {
        heading: String,
        #[serde(default)]
        lines: Vec<String>,
        #[serde(default)]
        narration: Option<String>,
    },
    Terminal {
        tag: String,
        #[serde(default)]
        tone: Tone,
        caption: String,
        lines: Vec<Line>,
        #[serde(default)]
        narration: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Tone {
    Before,
    After,
    #[default]
    Neutral,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Style {
    /// Typed character by character.
    Cmd,
    Out,
    Good,
    Bad,
    Dim,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Line {
    pub(crate) style: Style,
    pub(crate) text: String,
}

impl Scene {
    pub(crate) fn narration(&self) -> Option<&str> {
        match self {
            Self::Card { narration, .. } | Self::Terminal { narration, .. } => {
                narration.as_deref().filter(|n| !n.trim().is_empty())
            }
        }
    }

    /// How long the scene needs on screen without narration; the recorder's
    /// typing speed (22 ms a character) and pauses are mirrored here.
    pub(crate) fn natural_ms(&self) -> u64 {
        match self {
            Self::Card { lines, .. } => 3500 + 400 * lines.len() as u64,
            Self::Terminal { lines, .. } => {
                let shown: u64 = lines
                    .iter()
                    .map(|line| match line.style {
                        Style::Cmd => 22 * line.text.chars().count() as u64 + 500,
                        _ if line.text.is_empty() => 200,
                        _ => 650,
                    })
                    .sum();
                shown + 3000
            }
        }
    }
}

impl Storyboard {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let board: Self = serde_json::from_str(text).map_err(|e| format!("storyboard: {e}"))?;
        if board.scenes.is_empty() {
            return Err("storyboard: no scenes".into());
        }
        if board.voice.is_empty() || !board.voice.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(format!("storyboard: `{}` is not a voice name", board.voice));
        }
        for (index, scene) in board.scenes.iter().enumerate() {
            let number = index + 1;
            if scene
                .narration()
                .is_some_and(|n| n.chars().count() > TTS_MAX_CHARS)
            {
                return Err(format!(
                    "storyboard: scene {number}'s narration is over {TTS_MAX_CHARS} characters"
                ));
            }
        }
        Ok(board)
    }
}

/// Planned scene lengths: the natural time, stretched to cover the voice.
pub(crate) fn durations(board: &Storyboard, voices: &[Option<u64>]) -> Vec<u64> {
    board
        .scenes
        .iter()
        .enumerate()
        .map(|(i, scene)| {
            let voice = voices.get(i).copied().flatten();
            scene
                .natural_ms()
                .max(voice.map_or(0, |v| v.saturating_add(600)))
        })
        .collect()
}

pub(crate) fn check_duration(durations: &[u64]) -> Result<(), String> {
    let total = durations
        .iter()
        .fold(0_u64, |sum, duration| sum.saturating_add(*duration));
    if total > MAX_SECONDS * 1000 {
        return Err(format!(
            "the storyboard runs {}s; the longest level allows {MAX_SECONDS}s: cut it",
            total / 1000
        ));
    }
    Ok(())
}

pub(crate) fn measured_duration(timings: &Timings, scene_count: usize) -> Result<u64, String> {
    if timings.scenes_ms.len() != scene_count {
        return Err(format!(
            "the recorder measured {} scene(s) for a storyboard of {scene_count}",
            timings.scenes_ms.len()
        ));
    }
    let total = timings
        .scenes_ms
        .iter()
        .fold(timings.lead_in_ms, |sum, duration| {
            sum.saturating_add(*duration)
        });
    if total > MAX_SECONDS * 1000 {
        return Err(format!(
            "the recorded showcase runs {}s; the longest level allows {}s: cut it",
            total / 1000,
            MAX_SECONDS
        ));
    }
    Ok(total)
}

/// Resolve the requested video path beneath `.cache/showcase`, refusing
/// symlinks in every component and multiply-linked existing files.
pub(crate) fn resolve_out(root: &Path, requested: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let cache = ensure_directory(&root, &root.join(".cache"), &root)?;
    let base = ensure_directory(&root, &cache.join("showcase"), &root)?;
    let requested = Path::new(requested);
    if requested
        .components()
        .any(|part| part == Component::ParentDir)
    {
        return Err("SHOWCASE_OUT must not contain `..`".into());
    }
    let relative = if requested.is_absolute() {
        requested
            .strip_prefix(&base)
            .map_err(|_| "SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`")?
            .to_path_buf()
    } else {
        let parts: Vec<_> = requested.components().collect();
        if parts.starts_with(&[
            Component::Normal(std::ffi::OsStr::new(".cache")),
            Component::Normal(std::ffi::OsStr::new("showcase")),
        ]) {
            requested
                .strip_prefix(Path::new(".cache/showcase"))
                .map_err(|_| "SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`")?
                .to_path_buf()
        } else {
            return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into());
        }
    };
    if relative.as_os_str().is_empty() {
        return Err("SHOWCASE_OUT must name a file inside `<repo>/.cache/showcase/`".into());
    }
    let mut parent = base.clone();
    let mut file = None;
    let parts: Vec<_> = relative.components().collect();
    for (index, part) in parts.iter().enumerate() {
        match part {
            Component::Normal(name) if index + 1 == parts.len() => file = Some(name),
            Component::Normal(name) => {
                parent = ensure_directory(&base, &parent.join(name), &base)?;
            }
            Component::CurDir => {}
            _ => return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into()),
        }
    }
    let file = file.ok_or("SHOWCASE_OUT must name a file inside `<repo>/.cache/showcase/`")?;
    let output = parent.join(file);
    match fs::symlink_metadata(&output) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.nlink() > 1 {
                return Err(
                    "SHOWCASE_OUT must be a single-link regular file, not a symlink or directory"
                        .into(),
                );
            }
            let resolved = output
                .canonicalize()
                .map_err(|error| format!("cannot resolve SHOWCASE_OUT: {error}"))?;
            if !resolved
                .parent()
                .is_some_and(|parent| parent.starts_with(&base))
            {
                return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into());
            }
            Ok(resolved)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(output),
        Err(error) => Err(format!("cannot inspect SHOWCASE_OUT: {error}")),
    }
}

fn ensure_directory(scope: &Path, path: &Path, repository: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "SHOWCASE_OUT path component {} is a symlink",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
        }
        Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
    }
    let resolved = path
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    if !resolved.starts_with(scope) || !resolved.starts_with(repository) || !resolved.is_dir() {
        return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into());
    }
    Ok(resolved)
}

/// What the recorder measured: blank video before the first scene, then the
/// actual length of each scene (a slow page can overrun its plan).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Timings {
    pub(crate) lead_in_ms: u64,
    pub(crate) scenes_ms: Vec<u64>,
}

/// One piece of the narration track: a scene's voice-over (by scene index)
/// or silence, held for `ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) voice: Option<usize>,
    pub(crate) ms: u64,
}

/// The narration track that lines up with the recorded video.
pub(crate) fn track(timings: &Timings, voices: &[Option<u64>]) -> Result<Vec<Segment>, String> {
    if timings.scenes_ms.len() != voices.len() {
        return Err(format!(
            "the recorder measured {} scene(s) for a storyboard of {}",
            timings.scenes_ms.len(),
            voices.len()
        ));
    }
    let mut segments = vec![];
    if timings.lead_in_ms > 0 {
        segments.push(Segment {
            voice: None,
            ms: timings.lead_in_ms,
        });
    }
    for (index, (ms, voice)) in timings.scenes_ms.iter().zip(voices).enumerate() {
        segments.push(Segment {
            voice: voice.map(|_| index),
            ms: *ms,
        });
    }
    Ok(segments)
}
