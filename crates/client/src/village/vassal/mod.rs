//! Owner-visible domain presentation; economic decisions stay on the server.
use super::{Client, building_center, owner};
use crate::playground::game::map;
use aoe_protocol::{BuildingKind, ConstructionState, UnitKind};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function vassal_update(v, label) { window.aoeVassal?.update(v, label); }"
)]
extern "C" {
    fn vassal_update(v: &[f64], label: &str);
}

pub(super) fn present(client: &Client) {
    let player = owner(client);
    let mut points = Vec::new();
    let mut label = String::new();
    if let Some(b) = client.village.buildings.iter().find(|b| {
        Some(b.id) == client.selected.building_id
            && b.player == player
            && b.kind == BuildingKind::Castle
            && b.construction == ConstructionState::Complete
    }) {
        let vassal = b
            .garrison
            .iter()
            .find(|g| g.kind == UnitKind::Vassal && g.hp > 0);
        let radius = f64::from(aoe_simulation::VASSAL_RULES.radius_tiles);
        let center = building_center(b);
        points.extend([f64::from(u8::from(vassal.is_some())), radius]);
        let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
        let scene = map::scene::prepare(client);
        for index in 0..64 {
            let angle = std::f64::consts::TAU * f64::from(index) / 64.0;
            let world = [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ];
            let height = scene
                .height(world)
                .unwrap_or_else(|| map::elevation_at_world(client, world));
            let p = client.camera.world_to_screen_at_height(world, height);
            points.extend([p.x * scale, p.y * scale]);
        }
        label = if let Some(v) = vassal {
            let managed = client
                .units
                .values()
                .filter(|u| u.managed_by == Some(v.id))
                .count();
            format!("Vassal domain active · 100-tile radius · {managed} managed villagers")
        } else {
            "Garrison a Vassal here to activate its 100-tile domain".into()
        };
    } else if let Some(u) = client
        .selected
        .units
        .iter()
        .find_map(|id| client.units.get(id).filter(|u| u.player == player))
    {
        if u.managed_by.is_some() {
            label = "Managed by Vassal".into();
        } else if u.kind == UnitKind::Vassal {
            label = "Garrison in its home Castle to manage the economy within 100 tiles".into();
        }
    }
    vassal_update(&points, &label);
}
