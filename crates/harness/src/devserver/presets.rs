//! The maps the dev server keeps prepared: the requests the start screen
//! makes for its presets (`web/start/views.js`, `sequence.js`) and the maps
//! the compare bookmarks name, in the shape the server's `AOE_PRESET_MAPS`
//! file takes.
use crate::ship::compare::Bookmarks;
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, String>;

/// The start screen's presets: id, latitude, longitude (degrees), as in
/// `web/start/views.js` (a test holds the two together).
pub(crate) const PRESETS: &[(&str, f64, f64)] = &[
    ("paris", 48.8566, 2.3522),
    ("alps", 46.6, 10.5),
    ("brittany", 48.39, -4.49),
    ("finland", 61.5, 25.5),
];

/// The setup form's map sizes in kilometres (`#start-size`).
pub(crate) const SIZES: &[(&str, u64)] = &[
    ("tiny", 30),
    ("small", 120),
    ("large", 480),
    ("giant", 1920),
];

/// What the setup form submits for a map centred at the given e7 coordinates:
/// the form's default scale, seed and profile, automatic preparation.
pub(crate) fn request(latitude_e7: i64, longitude_e7: i64, side_meters: u64) -> Value {
    json!({
        "schema_version": 1,
        "center_latitude_e7": latitude_e7,
        "center_longitude_e7": longitude_e7,
        "requested_side_meters": side_meters,
        "compression": {"numerator": 30, "denominator": 1},
        "year_ce": 600,
        "seed": 1,
        "reconstruction_profile": "circa600_v1",
        "preparation": "automatic",
        "hydrology_mode": "automatic",
    })
}

/// The request the form submits for a preset at `kilometres` per side.
pub(crate) fn named_request(preset: &str, kilometres: u64) -> Option<Value> {
    let (_, latitude, longitude) = PRESETS.iter().find(|(id, ..)| *id == preset)?;
    Some(request(
        degrees_e7(*latitude),
        degrees_e7(*longitude),
        kilometres * 1000,
    ))
}

fn degrees_e7(degrees: f64) -> i64 {
    (degrees * 1e7).round() as i64
}

/// `finland:tiny`, one of [`PRESETS`] at one of [`SIZES`].
fn named(text: &str) -> Result<Value> {
    let (preset, size) = text
        .split_once(':')
        .ok_or_else(|| format!("preset `{text}`: use <preset>:<size>, e.g. finland:tiny"))?;
    let (_, latitude, longitude) = PRESETS
        .iter()
        .find(|(id, ..)| *id == preset)
        .ok_or_else(|| format!("preset `{preset}`: use paris, alps, brittany or finland"))?;
    let (_, km) = SIZES
        .iter()
        .find(|(id, _)| *id == size)
        .ok_or_else(|| format!("size `{size}`: use tiny, small, large or giant"))?;
    Ok(request(
        degrees_e7(*latitude),
        degrees_e7(*longitude),
        km * 1000,
    ))
}

/// The preset list: Finnish lakeland, tiny (what the showcase step library
/// films), the bookmarks' maps, then `extra` (`AOE_DEV_SERVER_PRESETS`,
/// comma separated `<preset>:<size>`); no request twice.
pub(crate) fn wanted(bookmarks: Option<&str>, extra: Option<&str>) -> Result<Vec<Value>> {
    let mut list = vec![named("finland:tiny")?];
    if let Some(text) = bookmarks {
        for map in Bookmarks::parse(text)?.maps.values() {
            list.push(request(
                map.center_latitude_e7,
                map.center_longitude_e7,
                map.requested_side_meters,
            ));
        }
    }
    for name in extra.unwrap_or("").split(',').filter(|n| !n.is_empty()) {
        list.push(named(name.trim())?);
    }
    let mut unique: Vec<Value> = Vec::new();
    for item in list {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    Ok(unique)
}

/// How many of `wanted` the server on `port` has prepared (a completed job
/// for that centre and side), and how many there are.
pub(crate) fn ready(port: u16, wanted: &[Value]) -> (usize, usize) {
    let jobs: Vec<Value> = super::process::http_get(port, "/maps/jobs")
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_default();
    let key = |v: &Value| {
        [
            "center_latitude_e7",
            "center_longitude_e7",
            "requested_side_meters",
        ]
        .map(|name| v.get(name).cloned())
    };
    let done = wanted
        .iter()
        .filter(|want| {
            jobs.iter().any(|job| {
                job.get("state").and_then(Value::as_str) == Some("completed")
                    && job.get("request").is_some_and(|r| key(r) == key(want))
            })
        })
        .count();
    (done, wanted.len())
}
