//! Browser scenes: the single origin they may film (the app under
//! development), the steps that drive it, and the plan handed to the
//! recorder. `AppOrigin::allows` is the reference rule that record.mjs applies
//! to every HTTP request and WebSocket; everything else is aborted.
use super::{MAX_SECONDS, Scene, Storyboard};
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

/// One action on the filmed page, written `{"click": "<selector>"}`,
/// `{"key": "Enter"}`, `{"text": "typed"}` or `{"wait_ms": 500}`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Step {
    Click(String),
    Key(String),
    Text(String),
    WaitMs(u32),
}

impl Step {
    /// The step's share of the planned scene, counted once: the recorder
    /// starts the hold only after the steps ran.
    pub(crate) fn planned_ms(&self) -> u64 {
        match self {
            Self::Click(_) | Self::Key(_) => 0,
            Self::Text(text) => TYPE_MS * text.chars().count() as u64,
            Self::WaitMs(ms) => u64::from(*ms),
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
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

/// The one origin browser scenes may reach, as the URL prefixes the recorder
/// lets through: `http://<host>:<port>/` and `ws://<host>:<port>/`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct AppOrigin {
    http: String,
    ws: String,
}

impl AppOrigin {
    /// A plain local HTTP origin with an explicit port: no credentials, path,
    /// query or fragment, so no other local service can be named.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        let refuse = || {
            format!(
                "SHOWCASE_APP_URL={value}: use http://127.0.0.1:<port>/ or http://localhost:<port>/, the app under development"
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

    /// SHOWCASE_APP_URL, defaulting to the `make dev` address.
    pub(crate) fn from_env() -> Result<Self, String> {
        let value = std::env::var("SHOWCASE_APP_URL")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_APP_URL.into());
        Self::parse(&value)
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

/// The app origin when the storyboard films it (each path checked against
/// it), else `None`: card and terminal takes need no network at all.
pub(crate) fn app_for(board: &Storyboard) -> Result<Option<AppOrigin>, String> {
    let mut paths = board.scenes.iter().filter_map(|scene| match scene {
        Scene::Browser { path, .. } => Some(path),
        _ => None,
    });
    let Some(first) = paths.next() else {
        return Ok(None);
    };
    let app = AppOrigin::from_env()?;
    for path in std::iter::once(first).chain(paths) {
        app.resolve(path)?;
    }
    Ok(Some(app))
}

#[derive(Debug, Serialize)]
pub(crate) struct Plan<'a> {
    pub(crate) title: &'a str,
    pub(crate) renderer: super::Renderer,
    pub(crate) app: Option<AppOrigin>,
    pub(crate) scenes: Vec<Planned<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Planned<'a> {
    #[serde(flatten)]
    pub(crate) scene: &'a Scene,
    pub(crate) duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
}

/// What record.mjs plays: each scene with its planned length (steps
/// included once) and, for browser scenes, the absolute URL to open.
pub(crate) fn plan<'a>(
    board: &'a Storyboard,
    planned: &[u64],
    app: Option<AppOrigin>,
) -> Result<Plan<'a>, String> {
    let mut scenes = Vec::with_capacity(board.scenes.len());
    for (scene, duration_ms) in board.scenes.iter().zip(planned) {
        let url = match (scene, &app) {
            (Scene::Browser { path, .. }, Some(app)) => Some(app.resolve(path)?),
            (Scene::Browser { .. }, None) => {
                return Err("browser scenes need the app origin".into());
            }
            _ => None,
        };
        scenes.push(Planned {
            scene,
            duration_ms: *duration_ms,
            url,
        });
    }
    Ok(Plan {
        title: &board.title,
        renderer: board.renderer,
        app,
        scenes,
    })
}
