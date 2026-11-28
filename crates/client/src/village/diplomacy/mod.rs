use super::{Client, orders};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{
    CommandResult, DiplomacyView, DiplomaticStance, GameplayClientMessage, ResourceKind,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function update_diplomacy(info, names, markets) { window.aoeDiplomacy?.update(info, names, markets); }
export function diplomacy_result(accepted) { window.aoeDiplomacy?.result(accepted); }
")]
extern "C" {
    fn update_diplomacy(info: &[u32], names: &js_sys::Array, markets: &[u32]);
    fn diplomacy_result(accepted: bool);
}

#[derive(Default)]
pub(super) struct Diplomacy {
    allies: Vec<PlayerId>,
    market: Option<EntityId>,
    pending: Option<u64>,
}

pub(super) fn attackable(client: &Client, player: PlayerId) -> bool {
    player != super::owner(client)
        && client
            .village
            .diplomacy
            .allies
            .binary_search(&player)
            .is_err()
}

pub(crate) fn sync(client: &mut Client, view: DiplomacyView) {
    client.village.diplomacy.market = view.tribute_market;
    client.village.diplomacy.allies.clear();
    let names = js_sys::Array::new();
    let mut rows = vec![
        u32::from(view.viewer.0),
        u32::from(view.tribute_market.is_some()),
        u32::from(view.tribute_fee_permyriad),
        client.config.width_tiles as u32,
        client.config.height_tiles as u32,
    ];
    for seat in view.seats {
        if seat.stance == DiplomaticStance::Ally {
            client.village.diplomacy.allies.push(seat.player);
        }
        if let Some(name) = seat.name {
            names.push(&JsValue::from_str(&name));
        } else {
            names.push(&JsValue::NULL);
        }
        let [r, g, b] = aoe_rendering::player_colour(seat.player, view.viewer, client.team_colour)
            .map(u32::from);
        rows.extend([
            u32::from(seat.player.0),
            seat.stance as u32,
            seat.offer as u32,
            r << 16 | g << 8 | b,
        ]);
    }
    let markets: Vec<_> = view
        .markets
        .into_iter()
        .flat_map(|market| {
            [
                u32::from(market.player.0),
                market.tile.x as u32,
                market.tile.y as u32,
            ]
        })
        .collect();
    update_diplomacy(&rows, &names, &markets);
}

pub(crate) fn clear() {
    update_diplomacy(&[], &js_sys::Array::new(), &[]);
}

pub(crate) fn acknowledge(client: &mut Client, sequence: u64, result: CommandResult) {
    if client.village.diplomacy.pending == Some(sequence) {
        client.village.diplomacy.pending = None;
        diplomacy_result(result == CommandResult::Accepted);
    }
}

pub(super) fn command(client: &mut Client, action: u32, code: u32, target: u32, amount: u32) {
    let Ok(target) = u16::try_from(target) else {
        return;
    };
    let player = PlayerId(target);
    let sequence = client.next_sequence;
    let message = if action == 21 {
        let Some(&stance) = [
            DiplomaticStance::Enemy,
            DiplomaticStance::Neutral,
            DiplomaticStance::Ally,
        ]
        .get(code as usize) else {
            return;
        };
        GameplayClientMessage::DiplomacyOrder {
            sequence,
            player,
            stance,
        }
    } else {
        let Some(&resource) = [
            ResourceKind::Food,
            ResourceKind::Wood,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ]
        .get(code as usize) else {
            return;
        };
        let Some(market) = client.village.diplomacy.market else {
            diplomacy_result(false);
            return;
        };
        GameplayClientMessage::TributeOrder {
            sequence,
            player,
            market,
            resource,
            amount,
        }
    };
    if orders::send(client, message) {
        client.village.diplomacy.pending = Some(sequence);
    } else {
        diplomacy_result(false);
    }
}
