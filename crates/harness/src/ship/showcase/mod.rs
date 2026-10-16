//! `make showcase`: the PR's showcase video from a storyboard
//! (docs/showcase.md). Scenes are title cards, scripted terminal
//! transcripts (tagged before/after) and browser scenes filming the app under
//! development, as built before or after the change. Playwright in the pinned browser image
//! records them in one take; narration is voiced
//! with Gemini Flash Lite TTS on OpenRouter and mixed in with ffmpeg from the
//! pinned ship-tools image. Each scene lasts at least as long as its voice.
mod bridge;
mod browser;
mod capture;
mod check;
mod media;
mod mounts;
mod narration;
mod origins;
mod output;
mod preparation;
mod scenes;
pub(crate) mod session;
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
pub(crate) use media::{concat_entry, publish_temp_output};
pub(crate) use narration::narrate;
#[cfg(test)]
pub(crate) use narration::{save_tts_response, tts_command};
pub(crate) use origins::Build;
#[cfg(test)]
pub(crate) use origins::Origins;
use output::open_directory;
pub(crate) use output::{ShowcaseOutput, resolve_out, showcase_tmp};
pub(crate) use scenes::{Compare, Figure, Reply, SessionStep, Unit};
#[cfg(test)]
pub(crate) use scenes::{Role, Verdict};
use serde::{Deserialize, Serialize};
pub(crate) use timing::{Segment, Timings, check_duration, durations, measured_duration, track};
#[cfg(test)]
pub(crate) use workdir::{create_work_dir, write_recorder};

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
    /// Page size of the take: 1280×720, or `large` for 1920×1080 scaled into
    /// the same 1280×720 video (behaviour that depends on a desktop window).
    #[serde(default)]
    pub(crate) viewport: Viewport,
    pub(crate) scenes: Vec<Scene>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Renderer {
    #[default]
    Default,
    Webgpu,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Viewport {
    #[default]
    Default,
    Large,
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
        #[serde(default)]
        lines: Vec<Line>,
        /// Two panes, before left and after right, instead of `lines`.
        #[serde(default)]
        compare: Option<Compare>,
        #[serde(default)]
        narration: Option<String>,
    },
    /// The same command before and after, as two bars racing.
    Timing {
        label: String,
        command: String,
        before_ms: u64,
        after_ms: u64,
        #[serde(default)]
        unit: Unit,
        #[serde(default)]
        extra: Vec<Figure>,
        #[serde(default)]
        narration: Option<String>,
    },
    /// A request and its response before and after, changed lines marked.
    Api {
        caption: String,
        method: String,
        path: String,
        #[serde(default)]
        request: Option<String>,
        before: Reply,
        after: Reply,
        #[serde(default)]
        narration: Option<String>,
    },
    /// A replayed headless agent session (`make showcase-session`).
    #[serde(rename = "agent-session")]
    AgentSession {
        tag: String,
        caption: String,
        steps: Vec<SessionStep>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bookmark: Option<String>,
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
            | Self::Timing { narration, .. }
            | Self::Api { narration, .. }
            | Self::AgentSession { narration, .. }
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
            Self::Terminal { lines, compare, .. } => match compare {
                Some(compare) => scenes::compare_ms(compare),
                None => scenes::lines_ms(lines) + scenes::HOLD_MS,
            },
            Self::Timing { extra, .. } => scenes::timing_ms(extra.len()),
            Self::Api {
                method,
                path,
                before,
                after,
                ..
            } => scenes::api_ms(method, path, before, after),
            Self::AgentSession { steps, .. } => scenes::session_ms(steps),
            // The steps run once, then the hold: navigation is not planned
            // (it is measured) and each step is counted exactly once.
            Self::Browser { seconds, steps, .. } => steps
                .iter()
                .map(Step::planned_ms)
                .fold(u64::from(*seconds) * 1000, u64::saturating_add),
        }
    }
}

impl Scene {
    /// The kind's own limits; the narration limit is checked by the board.
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Card { .. } => Ok(()),
            Self::Terminal { lines, compare, .. } => {
                scenes::validate_terminal(lines, compare.as_ref())
            }
            Self::Timing {
                label,
                command,
                before_ms,
                after_ms,
                extra,
                ..
            } => scenes::validate_timing(label, command, *before_ms, *after_ms, extra),
            Self::Api {
                caption,
                method,
                path,
                request,
                before,
                after,
                ..
            } => scenes::validate_api(caption, method, path, request.as_deref(), before, after),
            Self::AgentSession {
                tag,
                caption,
                steps,
                ..
            } => scenes::validate_session(tag, caption, steps),
            Self::Browser {
                path,
                bookmark,
                seconds,
                steps,
                ..
            } => {
                if let Some(name) = bookmark {
                    super::compare::bookmarks::identifier("bookmark name", name)?;
                    if path.as_deref().is_some_and(|path| path != "/") {
                        return Err("a bookmark permits only an omitted path or `/`".into());
                    }
                }
                let path = path
                    .as_deref()
                    .or(bookmark.as_ref().map(|_| "/"))
                    .ok_or("a browser scene needs a path or bookmark")?;
                browser::validate_scene(path, *seconds, steps)
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
            scene
                .validate()
                .map_err(|error| format!("storyboard: scene {number} {error}"))?;
        }
        Ok(board)
    }
}
