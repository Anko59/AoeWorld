//! On-demand owned allocation accounting, without retaining a sampling history.
use super::{fixture::ACTIVE, map};
use wasm_bindgen::prelude::*;

#[path = "memory/tests.rs"]
#[cfg(test)]
mod tests;

// Rust owns the counters; the browser encodes the on-demand JSON presentation.
// This keeps a second JSON codec and its generic error/number formatters out of
// gameplay WASM. Typed views are consumed synchronously and never retained.
#[wasm_bindgen(inline_js = "
const count_names = [
  'client_references', 'terrain_chunks', 'terrain_bytes', 'height_probes',
  'height_candidates', 'terrain_requests', 'scene_bytes', 'render_frame_bytes',
  'fog_bytes', 'units', 'unit_bytes', 'interpolation_bytes', 'buildings_and_ghosts',
  'animals', 'projectiles', 'resource_entries', 'resource_bytes', 'stump_effects',
  'outgoing_message_bytes', 'control_group_members', 'move_queue_bytes'
];
export function snapshot_json(backend, counts, names, renderer) {
  const value = {backend};
  count_names.forEach((name, i) => { value[name] = counts[i]; });
  value.renderer = names.map((name, i) => [name, renderer[i]]);
  return JSON.stringify(value);
}
")]
extern "C" {
    fn snapshot_json(
        backend: &str,
        counts: &[u32],
        names: &js_sys::Array,
        renderer: &[u32],
    ) -> String;
}

struct Snapshot {
    backend: &'static str,
    counts: [usize; 21],
    renderer: [(&'static str, usize); 5],
}

impl Snapshot {
    fn json(&self) -> String {
        let names = js_sys::Array::new();
        for (name, _) in self.renderer {
            names.push(&JsValue::from_str(name));
        }
        snapshot_json(
            self.backend,
            &self.counts.map(|value| value as u32),
            &names,
            &self.renderer.map(|(_, value)| value as u32),
        )
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
                super::controls::navigation::group_members(),
                client.village.move_queues.capacity()
                    * std::mem::size_of::<aoe_protocol::MoveQueue>()
                    + client
                        .village
                        .move_queues
                        .iter()
                        .map(|q| {
                            q.destinations.capacity()
                                * std::mem::size_of::<aoe_core::WorldPosition>()
                        })
                        .sum::<usize>(),
            ],
            renderer: client.renderer.memory_usage(),
        };
        Ok(value.json())
    })
}
