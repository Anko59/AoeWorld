//! The storyboard step library: named, parameterised steps that drive the
//! real game through the hooks its browser specs use (`web/start`,
//! `browser/tests/`), each with its own readiness wait. A step is expanded
//! here into a [`Script`] of primitive ops (script.rs) that `steps.mjs` runs;
//! a wait that times out fails the take naming the step and what it waited
//! for. Nothing here invents DOM: a step the client has no hook for is
//! refused when the storyboard is checked, before any image is built.
mod script;
pub(crate) mod warm;

use super::browser::Step;
use crate::ship::compare::{Bookmarks, DEFAULT_BOOKMARKS};
pub(crate) use script::{Script, expand};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Longest wait a step may ask for (ten minutes: a cold real-map start).
const MAX_TIMEOUT_MS: u32 = 600_000;
/// A start waits for a real map job; the other steps wait on the page.
pub(crate) const START_TIMEOUT_MS: u32 = 180_000;
pub(crate) const STEP_TIMEOUT_MS: u32 = 30_000;
/// The client clamps the camera to this zoom range (`playground_controls.rs`).
const ZOOM_RANGE: (f64, f64) = (0.25, 3.0);
const MAX_TILE: f64 = 1_000_000.0;
const MAX_NTH: u32 = 64;
const MAX_OFFSET_PX: i32 = 4096;
/// Where a pixel order lands relative to the unit, as in the feedback spec.
const DEFAULT_OFFSET: (i32, i32) = (128, 64);
const MAX_SELECTOR_CHARS: usize = 512;
/// The page the start steps need: the menu, with the intro skipped.
pub(crate) const MENU_PATH: &str = "/?skipIntro=1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Preset {
    Paris,
    Alps,
    Brittany,
    Finland,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Colour {
    Blue,
    Red,
    Green,
    Yellow,
    Cyan,
    Purple,
    Grey,
    Orange,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Size {
    Tiny,
    Small,
    Large,
    Giant,
}

impl Preset {
    /// The id of the preset button (`data-preset`) and of the dev server's presets.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Paris => "paris",
            Self::Alps => "alps",
            Self::Brittany => "brittany",
            Self::Finland => "finland",
        }
    }
}

impl Size {
    /// The side in kilometres, the value of the setup form's `#start-size`.
    pub(crate) fn km(self) -> u64 {
        match self {
            Self::Tiny => 30,
            Self::Small => 120,
            Self::Large => 480,
            Self::Giant => 1920,
        }
    }
}

/// The side the setup form starts with when a storyboard chooses none.
pub(crate) const DEFAULT_SIZE_KM: u64 = 480;

/// `{"start_skirmish": {"preset": "alps"}}`: Single Player, the preset's map,
/// Start Game, then waits for a running match with its chunks ready. The
/// setup form has no seed field (the request seed is fixed), so none is taken.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartSkirmish {
    pub(crate) preset: Preset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) colour: Option<Colour>,
    /// Map size; `tiny` (30 km) prepares fastest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) size: Option<Size>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_ms: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Unit,
    Villager,
    Building,
}

/// `{"select_entity": {"kind": "villager", "nth": 0}}`: centre the village
/// (`H`), then click the nth actor of that kind from `#scene data-actors`
/// (`villager`, `building` = the Town Center, `unit` = the scout). (Named apart
/// from the `select` step, which picks a `<select>` option.)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectEntity {
    pub(crate) kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) nth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_ms: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Action {
    Move,
    Gather,
    Build,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Resource {
    Food,
    Wood,
    Gold,
    Stone,
}

/// `{"command": {"action": "move", "dx": 128, "dy": 64}}`: a right click that
/// many canvas pixels from the canvas centre; or `{"action": "gather",
/// "resource": "wood"}`: frame the nearest resident node of that kind
/// (`#scene data-resource-targets`) and right-click it, then wait until a
/// villager works it (`#workers-wood`). Orders the current selection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Command {
    pub(crate) action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resource: Option<Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) dx: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) dy: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_ms: Option<u32>,
}

/// `{"camera": {"bookmark": "finland-start"}}` (from `qa/bookmarks.json`) or
/// `{"camera": {"x": 252, "y": 252, "zoom": 1.5}}`: the capture fragment, then
/// the camera read back and the view settled, as `make compare` does.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Camera {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bookmark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_ms: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    /// `#connection` reads `connected`.
    Connected,
    /// The match is on screen: `body[data-start-state=game]`, start screen gone.
    Game,
    /// No chunk pending and the minimap raster decoded.
    ChunksReady,
    /// Chunks ready and the frame quiet (compare's settle rule).
    Settled,
    /// Something is selected (`#selection` `data-count` above 0).
    Selected,
}

/// `{"wait_for": {"selector": "#minimap"}}` (visible) or
/// `{"wait_for": {"state": "settled"}}`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct WaitFor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) state: Option<State>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_ms: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Panel {
    /// The whole-map overview (`#overview-toggle`, `aria-expanded`).
    Overview,
    /// The isometric grid (`#grid`, `aria-pressed`).
    Grid,
}

/// `{"open_panel": {"name": "overview"}}`: opened if it is not already.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenPanel {
    pub(crate) name: Panel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) timeout_ms: Option<u32>,
}

fn check_timeout(timeout: Option<u32>) -> Result<(), String> {
    match timeout {
        Some(0) => Err("timeout_ms must be above 0".into()),
        Some(ms) if ms > MAX_TIMEOUT_MS => {
            Err(format!("timeout_ms is limited to {MAX_TIMEOUT_MS}"))
        }
        _ => Ok(()),
    }
}

impl StartSkirmish {
    pub(crate) fn validate(&self) -> Result<(), String> {
        check_timeout(self.timeout_ms)
    }
}

impl SelectEntity {
    pub(crate) fn validate(&self) -> Result<(), String> {
        check_timeout(self.timeout_ms)?;
        if self.nth.is_some_and(|n| n >= MAX_NTH) {
            return Err(format!("select_entity nth is below {MAX_NTH}"));
        }
        Ok(())
    }
}

impl Command {
    pub(crate) fn validate(&self) -> Result<(), String> {
        check_timeout(self.timeout_ms)?;
        match (self.action, self.resource, self.dx.or(self.dy)) {
            (Action::Build, ..) => {
                return Err("command action Build: the client has no build command yet (the command grid trains villagers, moves and stops), so use \"move\" or \"gather\"".into());
            }
            (Action::Move, Some(_), _) => {
                return Err("command move takes dx and dy, not a resource".into());
            }
            (Action::Gather, _, Some(_)) => {
                return Err("command gather takes a resource, not dx and dy".into());
            }
            _ => {}
        }
        let far = |v: Option<i32>| v.is_some_and(|v| v.abs() > MAX_OFFSET_PX);
        if far(self.dx) || far(self.dy) {
            return Err(format!(
                "command dx and dy are limited to ±{MAX_OFFSET_PX} px"
            ));
        }
        Ok(())
    }
}

impl Camera {
    pub(crate) fn validate(&self) -> Result<(), String> {
        check_timeout(self.timeout_ms)?;
        let coordinates = [self.x.is_some(), self.y.is_some(), self.zoom.is_some()];
        match (&self.bookmark, coordinates) {
            (Some(name), [false, false, false]) if !name.is_empty() => Ok(()),
            (None, [true, true, true]) => {
                let tile = |v: Option<f64>| v.is_some_and(|v| (0.0..=MAX_TILE).contains(&v));
                if !tile(self.x) || !tile(self.y) {
                    return Err(format!("camera x and y are tiles from 0 to {MAX_TILE}"));
                }
                match self.zoom {
                    Some(z) if (ZOOM_RANGE.0..=ZOOM_RANGE.1).contains(&z) => Ok(()),
                    _ => Err(format!(
                        "camera zoom is {} to {}",
                        ZOOM_RANGE.0, ZOOM_RANGE.1
                    )),
                }
            }
            _ => Err("camera takes a `bookmark`, or all of `x`, `y` and `zoom`".into()),
        }
    }

    /// Replaces a bookmark name by its coordinates, so the script needs no file.
    pub(crate) fn resolve(&mut self, bookmarks: &Bookmarks) -> Result<(), String> {
        let Some(name) = self.bookmark.take() else {
            return Ok(());
        };
        let mark = bookmarks
            .bookmarks
            .iter()
            .find(|mark| mark.name == name)
            .ok_or_else(|| format!("camera bookmark `{name}` is not in the bookmarks file"))?;
        (self.x, self.y, self.zoom) = (Some(mark.x), Some(mark.y), Some(mark.zoom));
        Ok(())
    }
}

impl WaitFor {
    pub(crate) fn validate(&self) -> Result<(), String> {
        check_timeout(self.timeout_ms)?;
        match (&self.selector, self.state) {
            (Some(s), None)
                if !s.trim().is_empty()
                    && s.chars().count() <= MAX_SELECTOR_CHARS
                    && !s.chars().any(char::is_control) =>
            {
                Ok(())
            }
            (None, Some(_)) => Ok(()),
            _ => Err(format!(
                "wait_for takes a `selector` of 1 to {MAX_SELECTOR_CHARS} printable characters, or a `state`, not both"
            )),
        }
    }
}

impl OpenPanel {
    pub(crate) fn validate(&self) -> Result<(), String> {
        check_timeout(self.timeout_ms)
    }
}

pub(crate) fn offset(command: &Command) -> (i32, i32) {
    (
        command.dx.unwrap_or(DEFAULT_OFFSET.0),
        command.dy.unwrap_or(DEFAULT_OFFSET.1),
    )
}

/// Replaces every `camera` bookmark name of the storyboard by its coordinates
/// from `qa/bookmarks.json`; the file is read only when a name needs it.
pub(crate) fn resolve_bookmarks(board: &mut super::Storyboard, root: &Path) -> Result<(), String> {
    let wanted = |step: &Step| matches!(step, Step::Camera(c) if c.bookmark.is_some());
    let mut bookmarks = None;
    for (index, scene) in board.scenes.iter_mut().enumerate() {
        let super::Scene::Browser { steps, .. } = scene else {
            continue;
        };
        for (position, step) in steps.iter_mut().enumerate() {
            if !wanted(step) {
                continue;
            }
            if bookmarks.is_none() {
                let path = root.join(DEFAULT_BOOKMARKS);
                let text = std::fs::read_to_string(&path).map_err(|error| {
                    format!("camera bookmarks need {}: {error}", path.display())
                })?;
                bookmarks = Some(Bookmarks::parse(&text)?);
            }
            let (Step::Camera(camera), Some(file)) = (step, &bookmarks) else {
                continue;
            };
            camera.resolve(file).map_err(|error| {
                format!(
                    "storyboard: scene {} step {}: {error}",
                    index + 1,
                    position + 1
                )
            })?;
        }
    }
    Ok(())
}
