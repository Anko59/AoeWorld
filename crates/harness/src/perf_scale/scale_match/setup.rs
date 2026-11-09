//! Creation of a new benchmark world.
use super::{
    BoxError, MapSource, Options, http,
    run::{ADMIN_TOKEN, Server},
    save,
};
use aoe_map::{MapRequest, Ratio};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) fn france_request(tiles: u32, seed: u64) -> Result<Value, BoxError> {
    let request = MapRequest {
        center_latitude_e7: 466_000_000,
        center_longitude_e7: 25_000_000,
        requested_side_meters: u64::from(tiles) * 2 * 30,
        compression: Ratio::new(30, 1)?,
        seed,
        ..MapRequest::default()
    };
    Ok(serde_json::to_value(request)?)
}

/// The `POST /match` body for these options.
pub(super) fn match_body(options: &Options) -> Result<Value, BoxError> {
    let mut slots = vec!["human"; options.humans];
    slots.resize(options.players, "ai");
    let mut body = json!({
        "slots": slots,
    });
    if options.start_villagers + options.start_militia > 0
        || options.march_probe
        || options.late_game
    {
        body["start_units"] =
            json!({ "villagers": options.start_villagers, "militia": options.start_militia });
    }
    if options.march_probe || options.late_game {
        body["start_units"]["probe_army"] = super::march::probe_army(options.march_probe);
        body["start_units"]["late_game"] = json!(options.late_game);
        body["seed"] = json!(options.seed);
    }
    match options.map {
        MapSource::Procedural => body["map"] = france_request(options.tiles, options.seed)?,
        MapSource::Package => {
            body["map_hash"] = json!(
                options
                    .map_hash
                    .as_deref()
                    .ok_or("--map package needs --map-hash")?
            );
        }
        MapSource::Flat => {
            body["flat"] = json!({ "tiles_per_side": options.tiles, "seed": options.seed });
        }
    }
    Ok(body)
}

/// Both paths return the map identity; saved seats remain owned by skirmish.
pub(super) fn start(
    options: &Options,
    server: &Server,
    saved: Option<&Value>,
) -> Result<(Value, Option<save::Human>, Option<Value>), BoxError> {
    let result = if let Some(card) = saved {
        let (human, status) = save::resume(server, card)?;
        (
            json!({"slots":[], "content_hash":card["map_hash"], "seeded_units":0, "partial_starting_kits":0}),
            Some(human),
            Some(status),
        )
    } else {
        let response = http::request(
            &server.address,
            "POST",
            "/match",
            &[("X-AoeWorld-Admin-Token", ADMIN_TOKEN)],
            Some(&serde_json::to_vec(&match_body(options)?)?),
            Duration::from_secs(60 * 60),
        )?;
        if response.status != 200 {
            return Err(format!(
                "POST /match: status {}: {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            )
            .into());
        }
        (serde_json::from_slice(&response.body)?, None, None)
    };
    Ok(result)
}
