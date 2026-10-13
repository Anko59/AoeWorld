//! `qa/bookmarks.json`: named camera positions on named maps, strictly parsed.
//! A map is named, never hashed: the same name resolves to whichever stored
//! package carries that request on each side of a comparison.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most views one run captures (each costs two screenshots and a settle).
pub(crate) const MAX_VIEWS: usize = 40;
/// Most bookmarks the file may hold.
pub(crate) const MAX_BOOKMARKS: usize = 200;
const MAX_MAPS: usize = 8;
const MAX_NAME: usize = 48;
const MAX_NOTE: usize = 240;
/// The client clamps the wheel zoom to this range (`playground_controls.rs`).
pub(crate) const ZOOM_RANGE: (f64, f64) = (0.25, 3.0);
const MAX_TILE: f64 = 1_000_000.0;

type Result<T> = std::result::Result<T, String>;

/// The stored package that a map name stands for: its request, which does not
/// change when a format change changes the package's content hash.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct MapRequest {
    pub(crate) center_latitude_e7: i64,
    pub(crate) center_longitude_e7: i64,
    pub(crate) requested_side_meters: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Renderer {
    #[default]
    Webgpu,
    Webgl2,
}

impl Renderer {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Webgpu => "webgpu",
            Self::Webgl2 => "webgl2",
        }
    }
}

fn verified() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Bookmark {
    pub(crate) name: String,
    /// A key of `maps`.
    pub(crate) map: String,
    /// Camera centre in tiles.
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) zoom: f64,
    #[serde(default)]
    pub(crate) renderer: Renderer,
    #[serde(default)]
    pub(crate) note: Option<String>,
    /// `false`: recorded by hand and not yet reproduced by `make compare`;
    /// skipped unless named in COMPARE_VIEWS.
    #[serde(default = "verified")]
    pub(crate) verified: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Bookmarks {
    pub(crate) maps: BTreeMap<String, MapRequest>,
    pub(crate) bookmarks: Vec<Bookmark>,
}

fn identifier(what: &str, value: &str) -> Result<()> {
    let ok = !value.is_empty()
        && value.len() <= MAX_NAME
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !value.starts_with('-');
    ok.then_some(()).ok_or_else(|| {
        format!("{what} `{value}`: use 1 to {MAX_NAME} of a-z, 0-9 and `-`, not starting with `-`")
    })
}

impl Bookmarks {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let file: Self = serde_json::from_str(text).map_err(|e| format!("bookmarks: {e}"))?;
        file.validate()?;
        Ok(file)
    }

    fn validate(&self) -> Result<()> {
        if self.maps.is_empty() || self.maps.len() > MAX_MAPS {
            return Err(format!("bookmarks: list 1 to {MAX_MAPS} maps"));
        }
        for name in self.maps.keys() {
            identifier("map name", name)?;
        }
        if self.bookmarks.is_empty() {
            return Err("bookmarks: no bookmarks".into());
        }
        if self.bookmarks.len() > MAX_BOOKMARKS {
            return Err(format!(
                "bookmarks: {} bookmarks, at most {MAX_BOOKMARKS}",
                self.bookmarks.len()
            ));
        }
        for (index, mark) in self.bookmarks.iter().enumerate() {
            identifier("bookmark name", &mark.name)?;
            if self.bookmarks[..index].iter().any(|m| m.name == mark.name) {
                return Err(format!("bookmarks: `{}` appears twice", mark.name));
            }
            if !self.maps.contains_key(&mark.map) {
                return Err(format!(
                    "bookmark `{}`: map `{}` is not in `maps`",
                    mark.name, mark.map
                ));
            }
            let tile = |v: f64| v.is_finite() && (0.0..=MAX_TILE).contains(&v);
            if !tile(mark.x) || !tile(mark.y) {
                return Err(format!(
                    "bookmark `{}`: x and y are tiles from 0 to {MAX_TILE}",
                    mark.name
                ));
            }
            if !(ZOOM_RANGE.0..=ZOOM_RANGE.1).contains(&mark.zoom) {
                return Err(format!(
                    "bookmark `{}`: zoom {} is outside the client's {}..={}",
                    mark.name, mark.zoom, ZOOM_RANGE.0, ZOOM_RANGE.1
                ));
            }
            if mark
                .note
                .as_deref()
                .is_some_and(|n| n.chars().count() > MAX_NOTE)
            {
                return Err(format!(
                    "bookmark `{}`: note is over {MAX_NOTE} characters",
                    mark.name
                ));
            }
        }
        Ok(())
    }

    /// The views of one run: COMPARE_VIEWS in the order given (any bookmark,
    /// verified or not), otherwise every verified bookmark in file order.
    pub(crate) fn select(&self, views: Option<&str>) -> Result<Vec<&Bookmark>> {
        let chosen: Vec<&Bookmark> = match views.filter(|v| !v.trim().is_empty()) {
            Some(list) => {
                let mut chosen: Vec<&Bookmark> = Vec::new();
                for name in list.split(',').map(str::trim) {
                    let mark = self
                        .bookmarks
                        .iter()
                        .find(|m| m.name == name)
                        .ok_or_else(|| {
                            let known: Vec<&str> =
                                self.bookmarks.iter().map(|m| m.name.as_str()).collect();
                            format!(
                                "COMPARE_VIEWS: no bookmark `{name}` (have {})",
                                known.join(", ")
                            )
                        })?;
                    if !chosen.iter().any(|m| m.name == mark.name) {
                        chosen.push(mark);
                    }
                }
                chosen
            }
            None => self.bookmarks.iter().filter(|m| m.verified).collect(),
        };
        if chosen.is_empty() {
            return Err(
                "no views selected: every bookmark is unverified; name them in COMPARE_VIEWS"
                    .into(),
            );
        }
        if chosen.len() > MAX_VIEWS {
            return Err(format!(
                "{} views selected, at most {MAX_VIEWS} per run: narrow COMPARE_VIEWS",
                chosen.len()
            ));
        }
        Ok(chosen)
    }
}
