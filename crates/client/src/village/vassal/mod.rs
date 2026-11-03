//! Owner-visible domain presentation; economic decisions stay on the server.
use super::{Client, building_center, owner};
use crate::playground::game::map;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{BuildingKind, ConstructionState, UnitKind};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

mod territory;

#[wasm_bindgen(
    inline_js = "export function vassal_update(v, label, settings) { window.aoeVassal?.update(v, label, settings); } export function vassal_bind(callback) { window.aoeVassal.bind(callback); } export function vassal_settings() { return window.aoeVassal.settings(); }"
)]
extern "C" {
    fn vassal_update(v: &[f64], label: &str, settings: &[u32]);
    fn vassal_settings() -> Vec<u8>;
    #[wasm_bindgen(catch)]
    fn vassal_bind(callback: &js_sys::Function) -> Result<(), JsValue>;
}

#[wasm_bindgen(inline_js = "export function general_update(v) { window.aoeGeneral?.update(v); }")]
extern "C" {
    fn general_update(v: &[f64]);
}

/// `[id, army, state, training, queued, mix * 5, counts * 5]`; the panel in
/// `web/interface/general-panel.js` reads exactly this layout.
pub(super) fn panel_values(id: EntityId, g: aoe_protocol::GeneralStatus) -> Vec<f64> {
    let state = match g.state {
        aoe_protocol::GeneralState::Idle => 0.0,
        aoe_protocol::GeneralState::Marching => 1.0,
        aoe_protocol::GeneralState::Fighting => 2.0,
        aoe_protocol::GeneralState::Retreating => 3.0,
    };
    let mut values = vec![
        f64::from(id.0),
        f64::from(g.army_size),
        state,
        f64::from(u8::from(g.training)),
        f64::from(g.queued),
    ];
    values.extend(g.mix.iter().map(|&n| f64::from(n)));
    values.extend(g.counts.iter().map(|&n| f64::from(n)));
    values
}

/// The panel's callback (action 9: Train on/off; action 10: slider, code =
/// role * 128 + percent) as a protocol command, given the current mix.
pub(super) fn panel_command(
    action: u32,
    code: u32,
    mix: [u8; 5],
) -> Option<aoe_protocol::GeneralCommand> {
    if action == 9 {
        return Some(aoe_protocol::GeneralCommand::SetTraining { enabled: code != 0 });
    }
    let (role, share) = ((code / 128) as usize, (code % 128).min(100) as u8);
    let mut mix = mix;
    *mix.get_mut(role)? = share;
    Some(aoe_protocol::GeneralCommand::SetMix { mix })
}

/// Values for the lone selected owned General, or empty (the panel hides).
fn general_panel(client: &Client, player: PlayerId) -> Vec<f64> {
    let mut selected = client.selected.units.iter();
    let (Some(id), None) = (selected.next(), selected.next()) else {
        return Vec::new();
    };
    client
        .units
        .get(id)
        .filter(|u| u.player == player && u.hp > 0)
        .and_then(|u| u.general)
        .map_or_else(Vec::new, |g| panel_values(*id, g))
}

/// The selected General's Train button or a mix slider.
pub(in super::super) fn general_command(client: &mut Client, action: u32, code: u32) {
    let Some(&id) = client.selected.units.first() else {
        return;
    };
    let Some(g) = client
        .units
        .get(&id)
        .filter(|u| u.player == owner(client) && u.hp > 0)
        .and_then(|u| u.general)
    else {
        return;
    };
    let Some(command) = panel_command(action, code, g.mix) else {
        return;
    };
    if let aoe_protocol::GeneralCommand::SetMix { mix } = command
        && let Some(general) = client.units.get_mut(&id).and_then(|u| u.general.as_mut())
    {
        // The sliders are the player's: show the change before the server echoes it.
        general.mix = mix;
    }
    let sequence = client.next_sequence;
    let message = aoe_protocol::GameplayClientMessage::GeneralOrder {
        sequence,
        entity_id: id,
        command,
    };
    super::orders::send(client, message);
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
    let limit = r.cap;
    let why = match r.wait {
        Growing => "training villagers",
        Full => "waiting: villager limit reached",
        Census => "waiting: too many units nearby to count",
        NoTownCenter => "waiting: no Town Center in its territory",
        Food => "waiting: not enough food",
        Population => "waiting: more housing needed",
        Paused => "waiting: villager training is off",
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
    let mut settings = Vec::new();
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
            if let Some(r) = v.vassal {
                settings.push(v.id.0);
                settings.extend(r.weights.map(u32::from));
                settings.extend([u32::from(r.cap), u32::from(r.training)]);
            }
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
            label = format!("General · {} soldiers · {state}", g.army_size);
        } else if u.commanded_by.is_some() {
            label = "Commanded by General".into();
        } else if u.managed_by.is_some() {
            label = "Managed by Vassal".into();
        } else if u.kind == UnitKind::Vassal {
            label = "Garrison in any owned Castle without a Vassal to manage its economy".into();
        }
    }
    vassal_update(&points, &label, &settings);
    general_update(&general_panel(client, player));
}

pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback = Closure::<dyn FnMut(u32)>::new(move |action| {
        let mut client = shared.borrow_mut();
        let Some(vassal_id) = super::production::selected(&client)
            .filter(|b| b.player == owner(&client) && b.kind == BuildingKind::Castle)
            .and_then(|b| {
                b.garrison
                    .iter()
                    .find(|g| g.kind == UnitKind::Vassal && g.hp > 0)
            })
            .map(|g| g.id)
        else {
            return;
        };
        let sequence = client.next_sequence;
        let message = if action == 1 {
            aoe_protocol::GameplayClientMessage::VassalDismissOrder {
                sequence,
                vassal_id,
            }
        } else {
            let values = vassal_settings();
            let [food, wood, gold, stone, cap, training] = values.as_slice() else {
                return;
            };
            aoe_protocol::GameplayClientMessage::VassalSettingsOrder {
                sequence,
                vassal_id,
                weights: [*food, *wood, *gold, *stone],
                cap: *cap,
                training: *training == 1,
            }
        };
        super::orders::send(&mut client, message);
    });
    vassal_bind(callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}
