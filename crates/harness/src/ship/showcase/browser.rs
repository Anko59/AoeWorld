//! Browser scenes: the single origin each may film (one build of the app
//! under development, origins.rs), the steps that drive it, and the plan
//! handed to the recorder. `AppOrigin::allows` is the reference rule that
//! record.mjs applies to every HTTP request and WebSocket of a scene with that
//! scene's own origin; everything else is aborted.
use super::{
    MAX_SECONDS, Scene, Storyboard,
    origins::{Bridged, Origins},
};
use serde::{Deserialize, Serialize};

/// `make dev` serves the app here (crates/harness/src/dev.rs).
pub(crate) const DEFAULT_APP_URL: &str = "http://127.0.0.1:8080/";
/// Typing speed of `text` steps, as in terminal scenes.
pub(crate) const TYPE_MS: u64 = 22;
const MAX_STEPS: usize = 64;
const MAX_WAIT_MS: u32 = 60_000;
const MAX_TEXT_CHARS: usize = 500;
const MAX_SELECTOR_CHARS: usize = 512;
const MAX_KEY_CHARS: usize = 32;
const MAX_PATH_BYTES: usize = 2048;
/// The recorder's viewport is 1280×720; a larger offset cannot land.
const MAX_POSITION_PX: u32 = 4096;

/// One action on the filmed page, written `{"click": "<selector>"}`,
/// `{"click_at": {"selector": "#minimap", "x": 37, "y": 63}}` (CSS pixels
/// from the element's top-left corner), `{"select": {"selector": "#map",
/// "value": "<option value>"}}`, `{"key": "Enter"}`, `{"text": "typed"}` or
/// `{"wait_ms": 500}`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Step {
    Click(String),
    ClickAt { selector: String, x: u32, y: u32 },
    Select { selector: String, value: String },
    Key(String),
    Text(String),
    WaitMs(u32),
}

impl Step {
    /// The step's share of the planned scene, counted once: the recorder
    /// starts the hold only after the steps ran.
    pub(crate) fn planned_ms(&self) -> u64 {
        match self {
            Self::Click(_) | Self::ClickAt { .. } | Self::Select { .. } | Self::Key(_) => 0,
            Self::Text(text) => TYPE_MS * text.chars().count() as u64,
            Self::WaitMs(ms) => u64::from(*ms),
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            Self::ClickAt { selector, x, y } => {
                Self::Click(selector.clone()).validate()?;
                if *x > MAX_POSITION_PX || *y > MAX_POSITION_PX {
                    return Err(format!(
                        "click positions are limited to {MAX_POSITION_PX} px"
                    ));
                }
                Ok(())
            }
            Self::Select { selector, value } => {
                Self::Click(selector.clone()).validate()?;
                if value.chars().count() > MAX_TEXT_CHARS || value.chars().any(char::is_control) {
                    return Err(format!(
                        "option values are limited to {MAX_TEXT_CHARS} printable characters"
                    ));
                }
                Ok(())
            }
            Self::Click(selector)
                if selector.trim().is_empty()
                    || selector.chars().count() > MAX_SELECTOR_CHARS
                    || selector.chars().any(char::is_control) =>
            {
                Err(format!(
                    "clicks need a selector of 1 to {MAX_SELECTOR_CHARS} printable characters"
                ))
            }
            Self::Key(key)
                if key.is_empty()
                    || key.len() > MAX_KEY_CHARS
                    || !key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"+_-".contains(&b)) =>
            {
                Err(format!("`{key}` is not a key name (e.g. Enter, Control+A)"))
            }
            Self::Text(text)
                if text.chars().count() > MAX_TEXT_CHARS || text.chars().any(char::is_control) =>
            {
                Err(format!(
                    "typed text is limited to {MAX_TEXT_CHARS} printable characters"
                ))
            }
            Self::WaitMs(ms) if *ms > MAX_WAIT_MS => {
                Err(format!("waits are limited to {MAX_WAIT_MS} ms"))
            }
            _ => Ok(()),
        }
    }
}

/// Storyboard checks that need no configuration: the path stays relative to
/// the app's origin, the hold is bounded and every step is well formed.
pub(crate) fn validate_scene(path: &str, seconds: u32, steps: &[Step]) -> Result<(), String> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.len() > MAX_PATH_BYTES
        || !path.chars().all(|c| c.is_ascii_graphic())
        || path.contains('\\')
    {
        return Err(format!(
            "films `{path}`: a browser path starts with one `/`, is printable ASCII without `\\` and at most {MAX_PATH_BYTES} bytes"
        ));
    }
    if seconds == 0 || u64::from(seconds) > MAX_SECONDS {
        return Err(format!("holds for {seconds}s; use 1 to {MAX_SECONDS}"));
    }
    if steps.len() > MAX_STEPS {
        return Err(format!(
            "has {} steps; the limit is {MAX_STEPS}",
            steps.len()
        ));
    }
    for (index, step) in steps.iter().enumerate() {
        step.validate()
            .map_err(|error| format!("step {}: {error}", index + 1))?;
    }
    Ok(())
}

/// The one origin a browser scene may reach, as the URL prefixes the recorder
/// lets through: `http://<host>:<port>/` and `ws://<host>:<port>/`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct AppOrigin {
    http: String,
    ws: String,
}

impl AppOrigin {
    /// A plain local HTTP origin with an explicit port: no credentials, path,
    /// query or fragment, so no other local service can be named.
    pub(crate) fn parse_named(variable: &str, value: &str) -> Result<Self, String> {
        let refuse = || {
            format!(
                "{variable}={value}: use http://127.0.0.1:<port>/ or http://localhost:<port>/, the app under development"
            )
        };
        let url = url::Url::parse(value).map_err(|_| refuse())?;
        let host = match url.host_str() {
            Some(host @ ("127.0.0.1" | "localhost")) => host,
            _ => return Err(refuse()),
        };
        let port = url.port().ok_or_else(refuse)?;
        if url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(refuse());
        }
        Ok(Self {
            http: format!("http://{host}:{port}/"),
            ws: format!("ws://{host}:{port}/"),
        })
    }

    /// [`Self::parse_named`] for SHOWCASE_APP_URL.
    #[cfg(test)]
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        Self::parse_named("SHOWCASE_APP_URL", value)
    }

    /// The app's HTTP prefix, e.g. `http://127.0.0.1:8080/`.
    pub(crate) fn http(&self) -> &str {
        &self.http
    }

    /// The app's loopback host and port, which the recorder's bridge dials.
    pub(crate) fn address(&self) -> Result<(String, u16), String> {
        let url = url::Url::parse(&self.http).map_err(|error| error.to_string())?;
        match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => Ok((host.to_owned(), port)),
            _ => Err(format!("{} names no host and port", self.http)),
        }
    }

    /// Whether the recorder lets `request` through: its normalized URL must
    /// start with the app's HTTP or WebSocket prefix.
    pub(crate) fn allows(&self, request: &str) -> bool {
        url::Url::parse(request).is_ok_and(|url| {
            let href = url.as_str();
            href.starts_with(&self.http) || href.starts_with(&self.ws)
        })
    }

    /// The absolute URL a scene's `path` opens, refused unless it stays on
    /// the app's origin.
    pub(crate) fn resolve(&self, path: &str) -> Result<String, String> {
        let url = url::Url::parse(&self.http)
            .and_then(|base| base.join(path))
            .map_err(|error| format!("`{path}`: {error}"))?;
        if url.scheme() != "http" || !self.allows(url.as_str()) {
            return Err(format!("`{path}` leaves {}", self.http));
        }
        Ok(url.into())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Plan<'a> {
    pub(crate) title: &'a str,
    pub(crate) renderer: super::Renderer,
    /// Every origin the take may reach, each served from its own socket;
    /// empty for card and terminal takes, which need no network at all.
    pub(crate) origins: Vec<Bridged>,
    pub(crate) scenes: Vec<Planned<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Planned<'a> {
    #[serde(flatten)]
    pub(crate) scene: &'a Scene,
    pub(crate) duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    /// The build tag drawn on a browser scene when both builds are filmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<&'static str>,
}

/// What record.mjs plays: each scene with its planned length (steps
/// included once) and, for browser scenes, the absolute URL to open on the
/// origin of the scene's own build.
pub(crate) fn plan<'a>(
    board: &'a Storyboard,
    planned: &[u64],
    origins: &Origins,
) -> Result<Plan<'a>, String> {
    let mut scenes = Vec::with_capacity(board.scenes.len());
    for (scene, duration_ms) in board.scenes.iter().zip(planned) {
        let (url, label) = match scene {
            Scene::Browser { build, path, .. } => (
                Some(origins.resolve(*build, path)?),
                origins.labelled().then(|| build.label()),
            ),
            _ => (None, None),
        };
        scenes.push(Planned {
            scene,
            duration_ms: *duration_ms,
            url,
            label,
        });
    }
    Ok(Plan {
        title: &board.title,
        renderer: board.renderer,
        origins: origins.bridged(),
        scenes,
    })
}
