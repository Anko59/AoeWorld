//! Project only boundary segments from the last complete walking shape.
use super::{BuildingKind, Client, ConstructionState, UnitKind, owner};
use crate::playground::game::map;
use aoe_core::{EntityId, TileCoord};
use aoe_protocol::OfficeZoneChunk;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function strategic_zone(id, vassal, edges) { window.aoeNavigation?.strategic?.zone(id, vassal, edges); }"
)]
extern "C" {
    fn strategic_zone(id: u32, vassal: bool, edges: &[i32]);
}

#[derive(Default)]
pub(crate) struct Zones {
    pending: BTreeMap<EntityId, Vec<OfficeZoneChunk>>,
    edges: BTreeMap<EntityId, Vec<[i32; 4]>>,
}

impl Zones {
    fn receive(
        &mut self,
        id: EntityId,
        start: bool,
        complete: bool,
        chunks: Vec<OfficeZoneChunk>,
    ) -> bool {
        if start && complete && chunks.is_empty() {
            self.pending.remove(&id);
            self.edges.remove(&id);
            return true;
        }
        if start {
            self.pending.insert(id, Vec::new());
        }
        let Some(pending) = self.pending.get_mut(&id) else {
            return false;
        };
        if pending
            .last()
            .zip(chunks.first())
            .is_some_and(|(last, first)| last.0 >= first.0)
        {
            self.pending.remove(&id);
            return false;
        }
        pending.extend(chunks);
        if complete && let Some(chunks) = self.pending.remove(&id) {
            self.edges.insert(id, outline(&chunks));
        }
        complete
    }
}

pub(in super::super::super) fn receive(
    client: &mut Client,
    id: EntityId,
    vassal: bool,
    start: bool,
    complete: bool,
    chunks: Vec<OfficeZoneChunk>,
) {
    let zones = &mut client.village.zones;
    if zones.receive(id, start, complete, chunks) {
        let edges = zones.edges.get(&id).map_or(&[][..], Vec::as_slice);
        strategic_zone(id.0, vassal, edges.as_flattened());
    }
}

fn outline(chunks: &[OfficeZoneChunk]) -> Vec<[i32; 4]> {
    let mut edges = Vec::new();
    let at = |x, y| {
        chunks
            .binary_search_by_key(&TileCoord::new(x, y), |(chunk, _)| *chunk)
            .ok()
            .map_or(&[0; 32], |index| &chunks[index].1)
    };
    for &(chunk, ref rows) in chunks {
        let (cx, cy) = (chunk.x, chunk.y);
        let west = at(cx - 1, cy);
        let east = at(cx + 1, cy);
        let north = at(cx, cy - 1)[31];
        let south = at(cx, cy + 1)[0];
        let x = cx * 32;
        for (y, &row) in rows.iter().enumerate() {
            let above = y.checked_sub(1).map_or(north, |i| rows[i]);
            let below = rows.get(y + 1).copied().unwrap_or(south);
            let world_y = cy * 32 + y as i32;
            for (mut mask, yy) in [(row & !above, world_y), (row & !below, world_y + 1)] {
                while mask != 0 {
                    let first = mask.trailing_zeros();
                    let last = first + (mask >> first).trailing_ones();
                    edges.push([x + first as i32, yy, x + last as i32, yy]);
                    mask &= u32::MAX.checked_shl(last).unwrap_or(0);
                }
            }
            let left = (row << 1) | (west[y] >> 31);
            let right = (row >> 1) | (east[y] << 31);
            for (mut mask, offset) in [(row & !left, 0), (row & !right, 1)] {
                while mask != 0 {
                    let xx = x + mask.trailing_zeros() as i32 + offset;
                    edges.push([xx, world_y, xx, world_y + 1]);
                    mask &= mask - 1;
                }
            }
        }
    }
    edges
}

/// The selected Town Center or Castle's branch of offices, in screen space.
/// Layout is [count, (vassal, selected, segment count, x0, y0, x1, y1, …)…].
pub(super) fn present(
    client: &Client,
    selected: EntityId,
    kind: BuildingKind,
    scale: f64,
) -> Vec<f64> {
    if crate::playground::game::strategic::active(client) || client.village.zones.edges.is_empty() {
        return Vec::new();
    }
    let vassal = kind == BuildingKind::Castle;
    let office = if vassal {
        UnitKind::Vassal
    } else {
        UnitKind::Steward
    };
    let player = owner(client);
    let scene = map::scene::prepare(client);
    let project = |x, y| {
        let world = [f64::from(x), f64::from(y)];
        let height = scene
            .height(world)
            .unwrap_or_else(|| map::elevation_at_world(client, world));
        let p = client.camera.world_to_screen_at_height(world, height);
        [p.x * scale, p.y * scale]
    };
    let mut values = vec![0.0];
    for building in &client.village.buildings {
        if building.player != player
            || building.kind != kind
            || building.construction != ConstructionState::Complete
        {
            continue;
        }
        for occupant in &building.garrison {
            if occupant.kind != office || occupant.hp == 0 {
                continue;
            }
            let Some(edges) = client.village.zones.edges.get(&occupant.id) else {
                continue;
            };
            let header = values.len();
            values.extend([
                f64::from(u8::from(vassal)),
                f64::from(u8::from(building.id == selected)),
                0.0,
            ]);
            for &[x, y, x1, y1] in edges {
                if client
                    .sent_region
                    .is_some_and(|r| x > r.max.x || y > r.max.y || x1 < r.min.x || y1 < r.min.y)
                {
                    continue;
                }
                let (x, x1) = client
                    .sent_region
                    .map_or((x, x1), |r| (x.max(r.min.x), x1.min(r.max.x)));
                // Re-sample a run at each tile vertex so it follows the
                // terrain under the boundary, including changes in height.
                let mut previous = project(x, y);
                for [next_x, next_y] in (x + 1..x1)
                    .map(|xx| [xx, y])
                    .chain(std::iter::once([x1, y1]))
                {
                    let next = project(next_x, next_y);
                    values.extend(previous);
                    values.extend(next);
                    previous = next;
                }
            }
            let count = (values.len() - header - 3) / 4;
            if count == 0 {
                values.truncate(header);
            } else {
                values[header + 2] = count as f64;
                values[0] += 1.0;
            }
        }
    }
    values
}

#[cfg(test)]
#[path = "territory_tests.rs"]
mod tests;
