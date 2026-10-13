//! What a run found, per view: the cameras read back on each side, the
//! changed-pixel count of the scene and the table printed at the end.
use super::{bookmarks::Renderer, pixels::Rect};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Two cameras are "the same view" within this many tiles and this zoom ratio.
const SAME_TILES: f64 = 0.5;
const SAME_ZOOM: f64 = 0.01;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Camera {
    pub(crate) world: [f64; 2],
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) zoom: f64,
}

impl Camera {
    pub(crate) fn same_view(&self, other: &Self) -> bool {
        (self.x - other.x).abs() <= SAME_TILES
            && (self.y - other.y).abs() <= SAME_TILES
            && (self.zoom / other.zoom - 1.0).abs() <= SAME_ZOOM
    }
}

/// `<name>.json` as `capture.mjs` writes it.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Capture {
    pub(crate) camera: Option<Camera>,
    pub(crate) chunks: Option<u64>,
    pub(crate) settle_ms: Option<u64>,
    pub(crate) scene: Option<Rect>,
    #[serde(default)]
    pub(crate) masks: Vec<Rect>,
    pub(crate) renderer: Option<String>,
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) page_errors: Vec<String>,
}

/// One side of one view, as observed.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Observed {
    pub(crate) camera: Camera,
    pub(crate) chunks_loaded: u64,
    pub(crate) settle_ms: u64,
    pub(crate) reported_renderer: Option<String>,
    pub(crate) page_errors: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Status {
    Identical,
    Changed,
    /// The two sides did not end on the same camera: no pixel verdict.
    CameraMismatch,
    /// A side could not be captured.
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Requested {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) zoom: f64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ViewReport {
    pub(crate) name: String,
    pub(crate) map: String,
    pub(crate) renderer: Renderer,
    pub(crate) requested: Requested,
    pub(crate) status: Status,
    pub(crate) before: Option<Observed>,
    pub(crate) after: Option<Observed>,
    pub(crate) scene: Option<Rect>,
    pub(crate) masks: Vec<Rect>,
    /// Pixels compared: the scene without the HUD rectangles over it.
    pub(crate) compared_pixels: Option<u64>,
    pub(crate) changed_pixels: Option<u64>,
    pub(crate) changed_percent: Option<f64>,
    /// Pixels that differ under the HUD rectangles (never in the count).
    pub(crate) masked_changed_pixels: Option<u64>,
    /// Zero scene pixels differ, only masked regions did.
    pub(crate) only_masked_differ: bool,
    pub(crate) error: Option<String>,
    pub(crate) files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SideInfo {
    pub(crate) source: String,
    pub(crate) revision: Option<String>,
    pub(crate) url: String,
    pub(crate) packages: String,
    pub(crate) map_hashes: BTreeMap<String, String>,
    /// Whether a gameplay controller session was opened on this server and
    /// whose it was.
    pub(crate) controller_lease: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) branch: String,
    pub(crate) head: String,
    pub(crate) base: String,
    pub(crate) viewport: [u32; 2],
    pub(crate) before: SideInfo,
    pub(crate) after: SideInfo,
    pub(crate) seconds: u64,
    pub(crate) views: Vec<ViewReport>,
    pub(crate) montage: Option<String>,
    /// The overall montage is shrunk by this whole factor (1 = 1:1).
    pub(crate) montage_shrink: u32,
}

fn cell(camera: Option<&Observed>) -> String {
    camera.map_or("-".into(), |o| {
        format!("{:.1},{:.1} z{:.3}", o.camera.x, o.camera.y, o.camera.zoom)
    })
}

/// The table printed at the end: one row per view.
pub(crate) fn table(views: &[ViewReport]) -> String {
    let rows: Vec<[String; 6]> = views
        .iter()
        .map(|v| {
            let verdict = match v.status {
                Status::Identical => "identical".to_owned(),
                Status::Changed if v.changed_pixels.is_some() => {
                    format!("{} px", v.changed_pixels.unwrap_or(0))
                }
                Status::Changed => "changed".to_owned(),
                Status::CameraMismatch => "camera mismatch".to_owned(),
                Status::Failed => format!("FAILED: {}", v.error.as_deref().unwrap_or("?")),
            };
            let percent = v
                .changed_percent
                .filter(|_| matches!(v.status, Status::Changed | Status::Identical))
                .map_or("-".into(), |p| format!("{p:.4}%"));
            let note = if v.only_masked_differ {
                "HUD only".to_owned()
            } else {
                String::new()
            };
            [
                v.name.clone(),
                v.renderer.name().to_owned(),
                verdict,
                percent,
                cell(v.after.as_ref().or(v.before.as_ref())),
                note,
            ]
        })
        .collect();
    let heads = [
        "view",
        "renderer",
        "changed",
        "of scene",
        "camera (tiles, zoom)",
        "",
    ];
    let mut widths = heads.map(str::len);
    for row in &rows {
        for (w, c) in widths.iter_mut().zip(row) {
            *w = (*w).max(c.chars().count());
        }
    }
    let line = |cells: &[String]| {
        cells
            .iter()
            .zip(widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_owned()
    };
    let mut out = vec![line(&heads.map(String::from))];
    out.extend(rows.iter().map(|r| line(r)));
    out.join("\n")
}
