//! Owner-visible domain presentation; economic decisions stay on the server.
use super::{Client, building_center, owner};
use crate::playground::game::map;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{BuildingKind, ConstructionState, UnitKind};
use wasm_bindgen::prelude::wasm_bindgen;

mod territory;

#[wasm_bindgen(
    inline_js = "export function vassal_update(v, label) { window.aoeVassal?.update(v, label); }"
)]
extern "C" {
    fn vassal_update(v: &[f64], label: &str);
}

/// Polygons (screen space) for every owned Castle's territory, laid out as
/// `[count, (active, selected, points, x0, y0, x1, y1, ...) * count]`.
fn domains(client: &Client, player: PlayerId, selected: EntityId) -> Vec<f64> {
    let castles: Vec<_> = client
        .village
        .buildings
        .iter()
        .filter(|b| {
            b.player == player
                && b.kind == BuildingKind::Castle
                && b.construction == ConstructionState::Complete
        })
        .map(|b| {
            (
                b.id,
                territory::Castle {
                    id: b.id,
                    center: building_center(b),
                    active: b
                        .garrison
                        .iter()
                        .any(|g| g.kind == UnitKind::Vassal && g.hp > 0),
                },
            )
        })
        .collect();
    let all: Vec<_> = castles.iter().map(|c| c.1).collect();
    let radius = f64::from(aoe_simulation::VASSAL_RULES.radius_tiles);
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    let scene = map::scene::prepare(client);
    let mut values = vec![castles.len() as f64];
    for (id, castle) in &castles {
        let ring = territory::outline(castle, &all, radius, 64);
        values.extend([
            f64::from(u8::from(castle.active)),
            f64::from(u8::from(*id == selected)),
            ring.len() as f64,
        ]);
        for world in ring {
            let height = scene
                .height(world)
                .unwrap_or_else(|| map::elevation_at_world(client, world));
            let p = client.camera.world_to_screen_at_height(world, height);
            values.extend([p.x * scale, p.y * scale]);
        }
    }
    values
}

/// One line saying how many villagers the Vassal manages and, when it is not
/// growing, why. Counts come from the server, not from the units in view.
fn describe(report: Option<aoe_protocol::VassalReport>) -> String {
    use aoe_protocol::VassalWait::*;
    let Some(r) = report else {
        return "Vassal domain active · evaluating".into();
    };
    let limit = aoe_simulation::VASSAL_RULES.max_villagers;
    let why = match r.wait {
        Growing => "training villagers",
        Full => "waiting: 100-villager limit reached",
        Census => "waiting: too many units nearby to count",
        NoTownCenter => "waiting: no Town Center in its territory",
        Population => "waiting: population cap (building Houses)",
        Food => "waiting: not enough food",
    };
    format!(
        "Vassal domain active · {}/{limit} villagers managed · {} in territory · {why}",
        r.managed, r.residents
    )
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
        points = domains(client, player, b.id);
        label = if let Some(v) = vassal {
            describe(v.vassal)
        } else {
            "Garrison a Vassal here to activate its 100-tile domain".into()
        };
    } else if let Some(u) = client
        .selected
        .units
        .iter()
        .find_map(|id| client.units.get(id).filter(|u| u.player == player))
    {
        if let Some(g) = u.general {
            let state = match g.state {
                aoe_protocol::GeneralState::Idle => "idle",
                aoe_protocol::GeneralState::Marching => "marching",
                aoe_protocol::GeneralState::Fighting => "fighting",
                aoe_protocol::GeneralState::Retreating => "retreating",
            };
            label = format!("General · {}/100 soldiers · {state}", g.army_size);
        } else if u.commanded_by.is_some() {
            label = "Commanded by General".into();
        } else if u.managed_by.is_some() {
            label = "Managed by Vassal".into();
        } else if u.kind == UnitKind::Vassal {
            label = "Garrison in its home Castle to manage the economy within 100 tiles".into();
        }
    }
    vassal_update(&points, &label);
}
