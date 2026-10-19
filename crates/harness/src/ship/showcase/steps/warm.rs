//! The readiness probe of a take that starts a skirmish: before the recorder
//! starts, every map its `start_skirmish` steps open is made ready on the
//! server they run against, i.e. its minimap raster is served once. The
//! server computes it on the first request under a 30 s budget and the client
//! never retries a 503 (devserver/warm.rs), so a take on a freshly started
//! server used to sit on the loading screen until its 180 s limit. A server
//! that has not prepared the map yet has nothing to warm: its own start does
//! that work.
use crate::devserver;
use crate::ship::showcase::{Step, browser::Plan};
use serde_json::Value;
use std::collections::BTreeMap;

/// The loopback port and storage origin, or `None` for any other host.
fn local_origin(url: &str) -> Option<(u16, String)> {
    let url = url::Url::parse(url).ok()?;
    let port = matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        .then(|| url.port_or_known_default())
        .flatten()?;
    Some((port, url.origin().ascii_serialization()))
}

/// Each distinct (port, map request) the plan's skirmish starts will open.
pub(crate) fn skirmish_maps(plan: &Plan<'_>) -> Vec<(u16, Value)> {
    let mut maps: Vec<(u16, Value)> = Vec::new();
    // record.mjs reuses one tab: sessionStorage survives scene navigation,
    // with a separate saved setup for each origin (web/start/sequence.js).
    let mut sizes = BTreeMap::new();
    for planned in &plan.scenes {
        let (Some(url), crate::ship::showcase::Scene::Browser { steps, .. }) =
            (planned.url.as_deref(), planned.scene)
        else {
            continue;
        };
        let Some((port, origin)) = local_origin(url) else {
            continue;
        };
        let km = sizes.entry(origin).or_insert(super::DEFAULT_SIZE_KM);
        for step in steps {
            if let Step::Select { selector, value } = step
                && selector == "#start-size"
                && let Ok(size) = value.parse::<u64>()
                && [30, 120, 480, 1920].contains(&size)
            {
                *km = size;
            }
            let Step::StartSkirmish(start) = step else {
                continue;
            };
            if let Some(size) = start.size {
                *km = size.km();
            }
            let Some(request) = devserver::skirmish_request(start.preset.id(), *km) else {
                continue;
            };
            if !maps.iter().any(|(p, r)| *p == port && *r == request) {
                maps.push((port, request));
            }
        }
    }
    maps
}

/// Make the maps of the plan's skirmish starts ready, saying what it did.
pub(crate) fn prepare(plan: &Plan<'_>) -> Result<(), String> {
    let maps = skirmish_maps(plan);
    let mut ports: Vec<u16> = maps.iter().map(|(port, _)| *port).collect();
    ports.dedup();
    for port in ports {
        let wanted: Vec<Value> = maps
            .iter()
            .filter(|(p, _)| *p == port)
            .map(|(_, request)| request.clone())
            .collect();
        let warmed = devserver::warm_maps(port, &wanted)
            .map_err(|why| format!("the server on port {port} cannot start the skirmish: {why}"))?;
        if warmed.is_empty() {
            println!(
                "showcase-phase map_warm none port {port}: no prepared map for the skirmish, the start prepares it"
            );
        }
        for (hash, took) in warmed {
            println!(
                "showcase-phase map_warm_ms {} port {port} map {}",
                took.as_millis(),
                &hash[..12.min(hash.len())]
            );
        }
    }
    Ok(())
}
