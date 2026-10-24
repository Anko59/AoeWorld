//! On-demand owned allocation accounting, without retaining a sampling history.
use super::{fixture::ACTIVE, map};
use wasm_bindgen::prelude::*;

#[path = "memory/tests.rs"]
#[cfg(test)]
mod tests;

// A fixed schema avoids pulling the recursive JSON Value serializer into
// gameplay WASM, and allocates no temporary object/map per observation.
struct Snapshot {
    backend: &'static str,
    counts: [usize; COUNT_NAMES.len()],
    renderer: [(&'static str, usize); 5],
}

const COUNT_NAMES: [&str; 19] = [
    "client_references",
    "terrain_chunks",
    "terrain_bytes",
    "height_probes",
    "height_candidates",
    "terrain_requests",
    "scene_bytes",
    "render_frame_bytes",
    "fog_bytes",
    "units",
    "unit_bytes",
    "interpolation_bytes",
    "buildings_and_ghosts",
    "animals",
    "projectiles",
    "resource_entries",
    "resource_bytes",
    "stump_effects",
    "outgoing_message_bytes",
];

impl serde::Serialize for Snapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(COUNT_NAMES.len() + 2))?;
        map.serialize_entry("backend", &self.backend)?;
        // Keep keys in static storage and serialize the counters in place,
        // without building a second array of key/value pairs on every call.
        for (key, value) in COUNT_NAMES.iter().zip(&self.counts) {
            map.serialize_entry(key, value)?;
        }
        map.serialize_entry("renderer", &self.renderer)?;
        map.end()
    }
}

/// Logical owned bytes, not allocator/driver overhead or process RSS. Browser
/// probes report JS heap, WASM linear memory and OS residency separately.
#[wasm_bindgen]
pub fn client_memory_snapshot() -> Result<String, JsValue> {
    ACTIVE.with(|active| {
        let shared = active
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
            .ok_or_else(|| JsValue::from_str("browser client is not initialized"))?;
        let client = shared.borrow();
        let chunks = client
            .terrain_chunks
            .values()
            .map(|c| c.resident_bytes())
            .sum::<usize>();
        let (probes, candidates) = client.terrain_bounds.memory_entries();
        let (resource_entries, resource_bytes, effects) = client.resources.memory_usage();
        let scene = client.terrain_scene.borrow();
        let scene_bytes = scene.as_ref().map_or(0, |s| s.resident_bytes());
        let value = Snapshot {
            backend: client.renderer.backend(),
            counts: [
                std::rc::Rc::strong_count(&shared),
                client.terrain_chunks.len(),
                chunks,
                probes,
                candidates,
                client.terrain_requests.len(),
                scene_bytes,
                client
                    .rendered_frame
                    .resident_bytes(map::scene::PreparedScene::resident_bytes),
                client.fog.resident_bytes(),
                client.units.len(),
                client.units.len() * std::mem::size_of::<aoe_protocol::GameplayUnitState>(),
                client.presentation.resident_bytes(),
                client.village.buildings.len(),
                client.village.animals.len(),
                client.village.projectiles.len(),
                resource_entries,
                resource_bytes,
                effects,
                client.socket.as_ref().map_or(0, |s| s.buffered_amount()) as usize,
            ],
            renderer: client.renderer.memory_usage(),
        };
        serde_json::to_string(&value).map_err(|error| JsValue::from_str(&error.to_string()))
    })
}
