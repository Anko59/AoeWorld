//! Owner-visible domain presentation; economic decisions stay on the server.
use super::{Client, owner};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{BuildingKind, ConstructionState, UnitKind};
use wasm_bindgen::prelude::wasm_bindgen;

mod mix;
mod territory;
pub(crate) use territory::Zones;
pub(in super::super) use territory::receive as receive_zone;
mod vassal;

#[wasm_bindgen(
    inline_js = "export function steward_update(v, label, settings) { window.aoeSteward?.update(v, label, settings); } export function steward_settings() { return window.aoeSteward.settings(); }"
)]
extern "C" {
    fn steward_update(v: &[f64], label: &str, settings: &[u32]);
    fn steward_settings() -> Vec<u8>;
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
    values.push(g.squads.iter().flatten().count() as f64);
    for squad in g.squads.into_iter().flatten() {
        values.extend([
            f64::from(squad.leader.0),
            squad.kind as u8 as f64,
            f64::from(squad.size),
            squad.state as u8 as f64,
        ]);
    }
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
    if action != 10 {
        return None;
    }
    let (role, share) = ((code / 128) as usize, (code % 128).min(100) as u8);
    let mix = mix::percentage(mix, role, share)?;
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

/// One line saying how many villagers the Steward manages and, when it is not
/// growing, why. Counts come from the server, not from the units in view.
fn describe(report: Option<aoe_protocol::StewardReport>) -> String {
    use aoe_protocol::StewardWait::*;
    let Some(r) = report else {
        return "Steward domain active · evaluating".into();
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
        "Steward domain active · {}/{limit} villagers managed · {} in territory · {why}",
        r.managed, r.residents
    )
}

pub(super) fn present(client: &Client, scale: f64) {
    if client.selected.units.len() + client.selected.buildings.len() > 1 {
        vassal::hide();
        steward_update(&[], "", &[]);
        general_update(&[]);
        return;
    }
    vassal::present(client);
    let player = owner(client);
    let mut label = String::new();
    let mut settings = Vec::new();
    let building = super::production::selected(client)
        .filter(|b| b.player == player && b.construction == ConstructionState::Complete);
    let points = building
        .filter(|b| matches!(b.kind, BuildingKind::TownCenter | BuildingKind::Castle))
        .map_or_else(Vec::new, |b| {
            territory::present(client, b.id, b.kind, scale)
        });
    if let Some(b) = building.filter(|b| b.kind == BuildingKind::TownCenter) {
        let steward = b
            .garrison
            .iter()
            .find(|g| g.kind == UnitKind::Steward && g.hp > 0);
        label = if let Some(v) = steward {
            if let Some(r) = v.steward {
                settings.push(v.id.0);
                settings.extend(r.weights.map(u32::from));
                settings.extend([u32::from(r.cap), u32::from(r.training)]);
            }
            describe(v.steward)
        } else {
            "Garrison a Steward here to activate its 100-tile domain".into()
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
            label = "Managed by Steward".into();
        } else if u.kind == UnitKind::Vassal {
            label = "Garrison this Vassal in a Castle to fortify and defend its territory".into();
        } else if u.kind == UnitKind::Steward {
            label =
                "Garrison in any owned Town Center without a Steward to manage its economy".into();
        }
    }
    steward_update(&points, &label, &settings);
    general_update(&general_panel(client, player));
}

pub(super) fn menu_order(client: &mut Client, code: u32) {
    if (2..=4).contains(&code) {
        vassal::order(client, code);
        return;
    }
    let Some(steward_id) = super::production::selected(client)
        .filter(|b| b.player == owner(client) && b.kind == BuildingKind::TownCenter)
        .and_then(|b| {
            b.garrison
                .iter()
                .find(|g| g.kind == UnitKind::Steward && g.hp > 0)
        })
        .map(|g| g.id)
    else {
        return;
    };
    let sequence = client.next_sequence;
    let message = if code == 1 {
        aoe_protocol::GameplayClientMessage::StewardDismissOrder {
            sequence,
            steward_id,
        }
    } else {
        let values = steward_settings();
        let [food, wood, gold, stone, cap, training] = values.as_slice() else {
            return;
        };
        aoe_protocol::GameplayClientMessage::StewardSettingsOrder {
            sequence,
            steward_id,
            weights: [*food, *wood, *gold, *stone],
            cap: *cap,
            training: *training == 1,
        }
    };
    super::orders::send(client, message);
}
