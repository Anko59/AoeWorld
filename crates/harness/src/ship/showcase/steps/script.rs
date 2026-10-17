//! Expands a library step into the primitive ops `steps.mjs` runs. Every
//! selector here is one the web client and its browser specs already use;
//! every wait carries the step's timeout and a plain description, so a
//! timeout names the step and what it was waiting for.
use super::{
    Action, Camera, Colour, Command, Kind, OpenPanel, Panel, Preset, Resource, START_TIMEOUT_MS,
    STEP_TIMEOUT_MS, SelectEntity, Size, StartSkirmish, State, WaitFor, offset,
};
use crate::ship::showcase::browser::Step;
use serde::Serialize;

const CONNECTED: &str = "#connection:text-is(\"connected\")";
const IN_GAME: &str = "body[data-start-state=\"game\"]";
const CHUNKS: &str = "#scene[data-chunks-pending=\"0\"]";
const RASTER: &str = "#minimap[data-raster=\"ready\"]";
const SELECTED: &str = "#selection:not([data-count=\"0\"])";
const HUD: &str = "#population:text-matches(\"^[0-9]+/[0-9]+$\")";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Verb {
    /// Click a selector, unless `unless` already matches.
    Click,
    Check,
    SelectOption,
    /// Click the canvas centre plus (dx, dy) CSS pixels; `right` for orders.
    ClickCanvas,
    /// Press a key (`value`).
    Key,
    /// Click the `nth` actor of kind `actor` in `#scene data-actors`.
    ClickActor,
    /// Frame the nearest resource node of `kind` and right-click it.
    Gather,
    /// Set the location hash (the capture fragment).
    Hash,
    /// Wait until the selector is visible.
    Wait,
    WaitHidden,
    /// Wait until the selector is in the document, shown or not.
    WaitAttached,
    /// Wait until `#minimap` reads back the camera (x, y, zoom).
    WaitCamera,
    /// Wait until the scene is settled (compare's rule).
    Settle,
}

/// One primitive. Only the fields its verb uses are set.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Op {
    pub(crate) op: Verb,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unless: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dx: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dy: Option<i32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) right: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) nth: Option<u32>,
    /// A resource kind index, as `data-resource-targets` numbers them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) camera: Option<[f64; 3]>,
    pub(crate) timeout_ms: u32,
    /// What the op waits for or does, as the error names it.
    pub(crate) what: String,
}

/// A step's ops under its label, e.g. `start_skirmish {"preset":"alps"}`.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Script {
    pub(crate) label: String,
    pub(crate) ops: Vec<Op>,
}

struct Builder {
    timeout_ms: u32,
    ops: Vec<Op>,
}

impl Builder {
    fn new(timeout: Option<u32>, default: u32) -> Self {
        Self {
            timeout_ms: timeout.unwrap_or(default),
            ops: Vec::new(),
        }
    }

    fn push(&mut self, op: Verb, what: &str) {
        self.ops.push(Op {
            op,
            selector: None,
            unless: None,
            value: None,
            dx: None,
            dy: None,
            right: false,
            actor: None,
            nth: None,
            kind: None,
            camera: None,
            timeout_ms: self.timeout_ms,
            what: what.to_owned(),
        });
    }

    /// Adjusts the op just pushed.
    fn tweak(&mut self, change: impl FnOnce(&mut Op)) {
        if let Some(op) = self.ops.last_mut() {
            change(op);
        }
    }

    fn on(&mut self, op: Verb, selector: &str, what: &str) {
        self.push(op, what);
        self.tweak(|op| op.selector = Some(selector.to_owned()));
    }

    fn wait(&mut self, selector: &str, what: &str) {
        self.on(Verb::Wait, selector, what);
    }

    fn connected(&mut self) {
        self.on(
            Verb::WaitAttached,
            CONNECTED,
            "the game connection to read `connected`",
        );
    }

    fn chunks_ready(&mut self) {
        self.wait(
            CHUNKS,
            "no terrain chunk pending (#scene data-chunks-pending=0)",
        );
        self.wait(RASTER, "the minimap raster ready (data-raster=ready)");
    }

    fn settle(&mut self) {
        self.push(
            Verb::Settle,
            "the view settled (chunks loaded, frame quiet)",
        );
    }

    fn in_game(&mut self) {
        self.wait(IN_GAME, "the match on screen (body data-start-state=game)");
        self.on(
            Verb::WaitHidden,
            "#start-sequence",
            "the start screens gone",
        );
        self.connected();
        self.wait(HUD, "the village HUD showing the population");
    }

    fn start(&mut self, start: &StartSkirmish) {
        self.on(Verb::Click, "#single-player", "the Single Player button");
        self.wait("#skirmish-setup:not([hidden])", "the Game Setup screen");
        self.connected();
        if let Some(colour) = start.colour {
            let name = colour_label(colour);
            self.on(
                Verb::Check,
                &format!("role=radio[name=\"{name}\"s]"),
                &format!("the {name} colour"),
            );
        }
        let preset = preset_id(start.preset);
        self.on(
            Verb::Click,
            &format!("[data-preset=\"{preset}\"]"),
            &format!("the {preset} preset"),
        );
        if let Some(size) = start.size {
            self.on(Verb::SelectOption, "#start-size", "the map size");
            self.tweak(|op| op.value = Some(size_km(size).to_owned()));
        }
        self.wait("#start-game:not([disabled])", "Start Game enabled");
        self.on(Verb::Click, "#start-game", "the Start Game button");
        self.in_game();
        self.chunks_ready();
        self.settle();
    }
}

fn preset_id(preset: Preset) -> &'static str {
    match preset {
        Preset::Paris => "paris",
        Preset::Alps => "alps",
        Preset::Brittany => "brittany",
        Preset::Finland => "finland",
    }
}

fn colour_label(colour: Colour) -> &'static str {
    match colour {
        Colour::Blue => "Blue",
        Colour::Red => "Red",
        Colour::Green => "Green",
        Colour::Yellow => "Yellow",
        Colour::Cyan => "Cyan",
        Colour::Purple => "Purple",
        Colour::Grey => "Grey",
        Colour::Orange => "Orange",
    }
}

fn size_km(size: Size) -> &'static str {
    match size {
        Size::Tiny => "30",
        Size::Small => "120",
        Size::Large => "480",
        Size::Giant => "1920",
    }
}

fn actor_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Unit => "Scout",
        Kind::Villager => "Villager",
        Kind::Building => "TownCenter",
    }
}

fn select(select: &SelectEntity) -> Builder {
    let mut b = Builder::new(select.timeout_ms, STEP_TIMEOUT_MS);
    // H centres the village on the Town Center and selects it; Escape clears.
    b.push(Verb::Key, "the village centred (H)");
    b.tweak(|op| op.value = Some("h".into()));
    b.chunks_ready();
    b.settle();
    b.push(Verb::Key, "the selection cleared (Escape)");
    b.tweak(|op| op.value = Some("Escape".into()));
    let kind = actor_kind(select.kind);
    b.push(
        Verb::ClickActor,
        &format!("a {kind} to appear in view and take the click"),
    );
    b.tweak(|op| (op.actor, op.nth) = (Some(kind.to_owned()), Some(select.nth.unwrap_or(0))));
    b.wait(SELECTED, "the selection panel to show the selection");
    b
}

fn command(command: &Command) -> Builder {
    let mut b = Builder::new(command.timeout_ms, STEP_TIMEOUT_MS);
    match command.action {
        Action::Gather => {
            let resource = command.resource.unwrap_or(Resource::Wood);
            let (index, name) = match resource {
                Resource::Food => (0, "food"),
                Resource::Wood => (1, "wood"),
                Resource::Gold => (2, "gold"),
                Resource::Stone => (3, "stone"),
            };
            b.push(
                Verb::Gather,
                &format!("the nearest {name} node framed and ordered"),
            );
            b.tweak(|op| (op.kind, op.right) = (Some(index), true));
            b.wait(
                &format!("#workers-{name}:text-matches(\"^[1-9]\")"),
                &format!("a villager working {name} (#workers-{name})"),
            );
        }
        _ => {
            let (dx, dy) = offset(command);
            b.push(Verb::ClickCanvas, "a right click ordering the move");
            b.tweak(|op| (op.dx, op.dy, op.right) = (Some(dx), Some(dy), true));
            b.wait(SELECTED, "the selection to stay after the order");
        }
    }
    b
}

fn camera(camera: &Camera) -> Result<Builder, String> {
    let (Some(x), Some(y), Some(zoom)) = (camera.x, camera.y, camera.zoom) else {
        let name = camera.bookmark.as_deref().unwrap_or("?");
        return Err(format!(
            "camera bookmark `{name}` was not resolved against the bookmarks file"
        ));
    };
    let mut b = Builder::new(camera.timeout_ms, STEP_TIMEOUT_MS);
    b.push(Verb::Hash, "the camera fragment");
    b.tweak(|op| op.value = Some(format!("x={x}&y={y}&zoom={zoom}")));
    b.push(Verb::WaitCamera, "the minimap to read back the camera");
    b.tweak(|op| op.camera = Some([x, y, zoom]));
    b.chunks_ready();
    b.settle();
    Ok(b)
}

fn wait_for(wait: &WaitFor) -> Builder {
    let mut b = Builder::new(wait.timeout_ms, STEP_TIMEOUT_MS);
    if let Some(selector) = &wait.selector {
        b.wait(selector, &format!("`{selector}` to be visible"));
    }
    match wait.state {
        Some(State::Connected) => b.connected(),
        Some(State::Game) => b.in_game(),
        Some(State::ChunksReady) => b.chunks_ready(),
        Some(State::Settled) => {
            b.chunks_ready();
            b.settle();
        }
        Some(State::Selected) => b.wait(SELECTED, "the selection panel to show a selection"),
        None => {}
    }
    b
}

fn panel(panel: &OpenPanel) -> Builder {
    let mut b = Builder::new(panel.timeout_ms, STEP_TIMEOUT_MS);
    let (toggle, open, what) = match panel.name {
        Panel::Creator => (
            "#creator-toggle",
            "#map-creator:not([hidden])",
            "the map creator",
        ),
        Panel::Overview => (
            "#overview-toggle",
            "#overview-toggle[aria-expanded=\"true\"]",
            "the map overview",
        ),
        Panel::Grid => ("#grid", "#grid[aria-pressed=\"true\"]", "the grid"),
    };
    b.on(Verb::Click, toggle, &format!("the {what} toggle"));
    b.tweak(|op| op.unless = Some(open.to_owned()));
    b.wait(open, &format!("{what} open"));
    b
}

fn label(step: &Step) -> String {
    let json = serde_json::to_value(step).unwrap_or_default();
    json.as_object()
        .and_then(|map| map.iter().next())
        .map_or_else(String::new, |(name, args)| format!("{name} {args}"))
}

/// The script of a library step; `None` for the primitive steps.
pub(crate) fn expand(step: &Step) -> Result<Option<Script>, String> {
    let builder = match step {
        Step::StartSkirmish(start) => {
            let mut b = Builder::new(start.timeout_ms, START_TIMEOUT_MS);
            b.start(start);
            b
        }
        Step::SelectEntity(entity) => select(entity),
        Step::Command(order) => command(order),
        Step::Camera(view) => camera(view)?,
        Step::WaitFor(wait) => wait_for(wait),
        Step::OpenPanel(open) => panel(open),
        _ => return Ok(None),
    };
    Ok(Some(Script {
        label: label(step),
        ops: builder.ops,
    }))
}
