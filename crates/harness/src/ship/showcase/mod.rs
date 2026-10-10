//! `make showcase`: the PR's showcase video from a storyboard
//! (docs/showcase.md). Scenes are title cards, scripted terminal
//! transcripts (tagged before/after) and browser scenes filming the app under
//! development, as built before or after the change. Playwright in the pinned browser image
//! records them in one take; narration is voiced
//! with Gemini Flash Lite TTS on OpenRouter and mixed in with ffmpeg from the
//! pinned ship-tools image. Each scene lasts at least as long as its voice.
mod bridge;
mod browser;
mod check;
mod media;
mod origins;
mod timing;
mod workdir;

#[cfg(test)]
pub(crate) use bridge::Bridge;
pub(crate) use browser::Step;
#[cfg(test)]
pub(crate) use browser::{AppOrigin, DEFAULT_APP_URL, plan};
pub(crate) use check::check;
#[cfg(test)]
pub(crate) use check::{check_voice_requirement, validate_image_value, validate_manifest_root};
pub(crate) use media::make;
#[cfg(test)]
pub(crate) use media::read_storyboard;
#[cfg(test)]
pub(crate) use media::{
    concat_entry, narrate, publish_temp_output, save_tts_response, tts_command,
};
pub(crate) use origins::Build;
#[cfg(test)]
pub(crate) use origins::Origins;
use serde::{Deserialize, Serialize};
use std::{
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};
pub(crate) use timing::{Segment, Timings, check_duration, durations, measured_duration, track};
#[cfg(test)]
pub(crate) use workdir::create_work_dir;

/// The validated output parent held open for the complete showcase run.
/// Publication uses only this directory descriptor, so replacing a path
/// component after validation cannot redirect the final write.
pub(crate) struct ShowcaseOutput {
    pub(crate) directory: std::fs::File,
    pub(crate) name: std::ffi::CString,
    pub(crate) path: std::path::PathBuf,
}

pub(crate) const TTS_MODEL: &str = "google/gemini-3.8-flash-lite-tts";
/// OpenRouter's limit on one speech request.
pub(crate) const TTS_MAX_CHARS: usize = 3000;
/// Gemini TTS on OpenRouter answers raw PCM only: 16-bit mono at 24 kHz.
pub(crate) const PCM_RATE: u32 = 24_000;
/// Narration may exceed the video cap by at most one second while rounding
/// PCM response durations to whole milliseconds.
pub(crate) const TTS_PCM_BUDGET: u64 = PCM_RATE as u64 * 2 * (MAX_SECONDS + 1);

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
    /// Browser for browser scenes: the default build, or Chromium with the
    /// same software WebGPU flags as the `webgpu` browser-test project.
    #[serde(default)]
    pub(crate) renderer: Renderer,
    pub(crate) scenes: Vec<Scene>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Renderer {
    #[default]
    Default,
    Webgpu,
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
    /// The app under development at `path` on the single origin of its
    /// `build`: the pull request's (SHOWCASE_APP_URL) unless it names the one
    /// before the change (SHOWCASE_BEFORE_URL). Driven by `steps`, then held
    /// for `seconds`.
    Browser {
        #[serde(default)]
        build: Build,
        path: String,
        caption: String,
        seconds: u32,
        #[serde(default)]
        steps: Vec<Step>,
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
            Self::Card { narration, .. }
            | Self::Terminal { narration, .. }
            | Self::Browser { narration, .. } => {
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
            // The steps run once, then the hold: navigation is not planned
            // (it is measured) and each step is counted exactly once.
            Self::Browser { seconds, steps, .. } => steps
                .iter()
                .map(Step::planned_ms)
                .fold(u64::from(*seconds) * 1000, u64::saturating_add),
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
            if let Scene::Browser {
                path,
                seconds,
                steps,
                ..
            } = scene
            {
                browser::validate_scene(path, *seconds, steps)
                    .map_err(|error| format!("storyboard: scene {number} {error}"))?;
            }
        }
        Ok(board)
    }
}

/// Resolve the requested video path beneath `.cache/showcase`, refusing
/// symlinks in every component and multiply-linked existing files.
pub(crate) fn resolve_out(root: &Path, requested: &str) -> Result<ShowcaseOutput, String> {
    let requested_path = Path::new(requested);
    if !requested_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".webm"))
    {
        return Err("SHOWCASE_OUT must end in `.webm`".into());
    }
    if !super::github::attachable_video(requested_path) {
        return Err("SHOWCASE_OUT filename must use only letters, digits, `.`, `_` or `-`".into());
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let root_fd =
        open_directory(&root).map_err(|error| format!("cannot open repository root: {error}"))?;
    let cache_fd = open_or_create_child(&root_fd, ".cache")
        .map_err(|error| format!("cannot open .cache: {error}"))?;
    let base_fd = open_or_create_child(&cache_fd, "showcase")
        .map_err(|error| format!("cannot open .cache/showcase: {error}"))?;
    let base = root.join(".cache/showcase");
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
    let mut parent_path = base.clone();
    let mut parent_fd = base_fd;
    let mut file = None;
    let parts: Vec<_> = relative.components().collect();
    for (index, part) in parts.iter().enumerate() {
        match part {
            Component::Normal(name) if index + 1 == parts.len() => file = Some(name),
            Component::Normal(name) => {
                let component = name.to_str().ok_or("SHOWCASE_OUT path must be UTF-8")?;
                parent_fd = open_or_create_child(&parent_fd, component)
                    .map_err(|error| format!("cannot open output directory: {error}"))?;
                parent_path.push(name);
            }
            Component::CurDir => {}
            _ => return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into()),
        }
    }
    let file = file.ok_or("SHOWCASE_OUT must name a file inside `<repo>/.cache/showcase/`")?;
    let name = std::ffi::CString::new(file.as_bytes())
        .map_err(|_| "SHOWCASE_OUT filename contains a NUL byte")?;
    match nix::sys::stat::fstatat(
        &parent_fd,
        name.as_c_str(),
        nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW,
    ) {
        Ok(metadata)
            if metadata.st_mode & nix::libc::S_IFMT != nix::libc::S_IFREG
                || metadata.st_nlink > 1 =>
        {
            return Err(
                "SHOWCASE_OUT must be a single-link regular file, not a symlink or directory"
                    .into(),
            );
        }
        Ok(_) => {}
        Err(nix::errno::Errno::ENOENT) => {}
        Err(error) => return Err(format!("cannot inspect SHOWCASE_OUT: {error}")),
    }
    Ok(ShowcaseOutput {
        directory: parent_fd,
        name,
        path: parent_path.join(file),
    })
}

/// Create and open the private temporary parent used by showcase runs.
pub(crate) fn showcase_tmp(root: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let root_fd =
        open_directory(&root).map_err(|error| format!("cannot open repository root: {error}"))?;
    let cache_fd = open_or_create_child(&root_fd, ".cache")
        .map_err(|error| format!("cannot open .cache: {error}"))?;
    let tmp_fd = open_or_create_private_child(&cache_fd, "tmp")
        .map_err(|error| format!("cannot open .cache/tmp: {error}"))?;
    nix::sys::stat::fchmod(&tmp_fd, nix::sys::stat::Mode::S_IRWXU)
        .map_err(|error| format!("cannot secure .cache/tmp: {error}"))?;
    let stat = nix::sys::stat::fstat(&tmp_fd)
        .map_err(|error| format!("cannot inspect .cache/tmp: {error}"))?;
    if stat.st_uid != nix::unistd::getuid().as_raw() {
        return Err(".cache/tmp must be owned by the current user".into());
    }
    Ok(root.join(".cache/tmp"))
}

fn open_directory(path: &Path) -> nix::Result<std::fs::File> {
    let fd = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )?;
    Ok(std::fs::File::from(fd))
}

fn open_or_create_child(parent: &std::fs::File, name: &str) -> nix::Result<std::fs::File> {
    let name = std::ffi::CString::new(name).expect("validated component has no NUL");
    match nix::fcntl::openat(
        parent,
        name.as_c_str(),
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => Ok(std::fs::File::from(fd)),
        Err(nix::errno::Errno::ENOENT) => {
            match nix::sys::stat::mkdirat(
                parent,
                name.as_c_str(),
                nix::sys::stat::Mode::S_IRWXU
                    | nix::sys::stat::Mode::S_IRGRP
                    | nix::sys::stat::Mode::S_IXGRP
                    | nix::sys::stat::Mode::S_IROTH
                    | nix::sys::stat::Mode::S_IXOTH,
            ) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(error) => return Err(error),
            }
            let fd = nix::fcntl::openat(
                parent,
                name.as_c_str(),
                nix::fcntl::OFlag::O_RDONLY
                    | nix::fcntl::OFlag::O_DIRECTORY
                    | nix::fcntl::OFlag::O_NOFOLLOW
                    | nix::fcntl::OFlag::O_CLOEXEC,
                nix::sys::stat::Mode::empty(),
            )?;
            Ok(std::fs::File::from(fd))
        }
        Err(error) => Err(error),
    }
}

fn open_or_create_private_child(parent: &std::fs::File, name: &str) -> nix::Result<std::fs::File> {
    let name = std::ffi::CString::new(name).expect("validated component has no NUL");
    let flags = nix::fcntl::OFlag::O_RDONLY
        | nix::fcntl::OFlag::O_DIRECTORY
        | nix::fcntl::OFlag::O_NOFOLLOW
        | nix::fcntl::OFlag::O_CLOEXEC;
    match nix::fcntl::openat(
        parent,
        name.as_c_str(),
        flags,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => Ok(std::fs::File::from(fd)),
        Err(nix::errno::Errno::ENOENT) => {
            match nix::sys::stat::mkdirat(parent, name.as_c_str(), nix::sys::stat::Mode::S_IRWXU) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(error) => return Err(error),
            }
            let fd = nix::fcntl::openat(
                parent,
                name.as_c_str(),
                flags,
                nix::sys::stat::Mode::empty(),
            )?;
            Ok(std::fs::File::from(fd))
        }
        Err(error) => Err(error),
    }
}
