//! Before and after: the two builds a browser scene may film and the origin
//! of each. `after` is the pull request's app (SHOWCASE_APP_URL); `before` is
//! the same app built from the base branch and served on another port
//! (SHOWCASE_BEFORE_URL). Each origin a storyboard films is bridged through
//! its own socket, and a scene reaches only its own build's origin.
use super::{
    Scene, Storyboard,
    browser::{AppOrigin, DEFAULT_APP_URL},
};
use serde::{Deserialize, Serialize};

const APP_URL: &str = "SHOWCASE_APP_URL";
const BEFORE_URL: &str = "SHOWCASE_BEFORE_URL";

/// Which build of the app a browser scene films; omitted means `after`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Build {
    Before,
    #[default]
    After,
}

impl Build {
    /// The bridge socket's name in the work directory, which record.mjs dials.
    pub(crate) fn socket(self) -> &'static str {
        match self {
            Self::Before => "before.sock",
            Self::After => "app.sock",
        }
    }

    /// The tag overlaid on the scene when a storyboard films both builds.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Before => "BEFORE (dev)",
            Self::After => "AFTER (this PR)",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// One origin the recorder serves inside its container: the port record.mjs
/// listens on and the socket it pipes that port's connections to.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Bridged {
    pub(crate) build: Build,
    #[serde(flatten)]
    pub(crate) origin: AppOrigin,
    pub(crate) socket: &'static str,
}

/// The origins a storyboard films: none for cards and terminals, otherwise
/// only the builds its browser scenes name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Origins {
    after: Option<AppOrigin>,
    before: Option<AppOrigin>,
}

impl Origins {
    /// Choose the origins `board` needs from the two settings. The app origin
    /// is validated whenever a browser scene exists; a `before` scene needs a
    /// second valid origin on another port, since the recorder serves both
    /// ports on one loopback address. Every scene path must stay on its origin.
    pub(crate) fn select(
        board: &Storyboard,
        app: &str,
        before: Option<&str>,
    ) -> Result<Self, String> {
        let scenes: Vec<(usize, Build, &str)> = board
            .scenes
            .iter()
            .enumerate()
            .filter_map(|(index, scene)| match scene {
                Scene::Browser { build, path, .. } => Some((index + 1, *build, path.as_str())),
                _ => None,
            })
            .collect();
        if scenes.is_empty() {
            return Ok(Self::default());
        }
        let app = AppOrigin::parse_named(APP_URL, app)?;
        let first_before = scenes
            .iter()
            .find(|(_, build, _)| *build == Build::Before)
            .map(|(number, ..)| *number);
        let before = match (first_before, before.filter(|value| !value.is_empty())) {
            (None, _) => None,
            (Some(number), None) => {
                return Err(format!(
                    "storyboard: scene {number} films the build before the change: set {BEFORE_URL} to that build's origin, e.g. http://127.0.0.1:8082/ (docs/showcase.md)"
                ));
            }
            (Some(_), Some(value)) => {
                let before = AppOrigin::parse_named(BEFORE_URL, value)?;
                if before.address()?.1 == app.address()?.1 {
                    return Err(format!(
                        "{BEFORE_URL}={value}: the build before the change must be served on another port than {APP_URL} ({})",
                        app.http()
                    ));
                }
                Some(before)
            }
        };
        let films_after = scenes.iter().any(|(_, build, _)| *build == Build::After);
        let origins = Self {
            after: films_after.then_some(app),
            before,
        };
        for (_, build, path) in scenes {
            origins.resolve(build, path)?;
        }
        Ok(origins)
    }

    /// SHOWCASE_APP_URL (default: the `make dev` address) and
    /// SHOWCASE_BEFORE_URL.
    pub(crate) fn from_env(board: &Storyboard) -> Result<Self, String> {
        let app = std::env::var(APP_URL)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_APP_URL.into());
        let before = std::env::var(BEFORE_URL).ok();
        Self::select(board, &app, before.as_deref())
    }

    /// The origin scenes of `build` film, when the storyboard has any.
    pub(crate) fn of(&self, build: Build) -> Option<&AppOrigin> {
        match build {
            Build::Before => self.before.as_ref(),
            Build::After => self.after.as_ref(),
        }
    }

    /// The absolute URL a `build` scene's `path` opens.
    pub(crate) fn resolve(&self, build: Build, path: &str) -> Result<String, String> {
        self.of(build)
            .ok_or_else(|| format!("a {} browser scene needs its origin", build.name()))?
            .resolve(path)
    }

    /// Every origin to bridge, each with its own socket.
    pub(crate) fn bridged(&self) -> Vec<Bridged> {
        [Build::After, Build::Before]
            .into_iter()
            .filter_map(|build| {
                self.of(build).map(|origin| Bridged {
                    build,
                    origin: origin.clone(),
                    socket: build.socket(),
                })
            })
            .collect()
    }

    /// Whether any scene of the take may reach `request`: exactly the
    /// configured origins. A single scene is held to its own build's origin
    /// alone ([`AppOrigin::allows`]), which is the rule the recorder applies;
    /// this union is the reference the tests hold the take to.
    #[cfg(test)]
    pub(crate) fn allows(&self, request: &str) -> bool {
        [&self.after, &self.before]
            .into_iter()
            .flatten()
            .any(|origin| origin.allows(request))
    }

    /// Whether the storyboard films both builds, so each scene is tagged.
    pub(crate) fn labelled(&self) -> bool {
        self.after.is_some() && self.before.is_some()
    }
}
