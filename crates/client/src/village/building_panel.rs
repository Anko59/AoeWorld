//! Bounded numeric building presentation; no DOM rules duplicate the simulation.
use super::{Client, construction, orders::send, owner};
use crate::{
    building_ui::{
        self,
        availability::{self, Availability},
    },
    playground::game::map,
};
use aoe_protocol::{ConstructionState, GameplayClientMessage as Message, Tech};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

#[wasm_bindgen(
    inline_js = "export function buildings_update(v) { window.aoeBuildings?.update(v); } export function placement_update(v) { window.aoeBuildings?.preview(v); } export function commands_bind(callback) { window.aoeBuildings.bind(callback); }"
)]
extern "C" {
    fn buildings_update(v: &[f64]);
    fn placement_update(v: &[f64]);
    #[wasm_bindgen(catch)]
    fn commands_bind(callback: &js_sys::Function) -> Result<(), JsValue>;
}

#[inline(never)]
pub(super) fn update(client: &Client) {
    let player = client
        .village
        .players
        .iter()
        .find(|p| p.player == owner(client));
    let reserved = client
        .village
        .buildings
        .iter()
        .filter(|b| b.player == owner(client))
        .map(|b| b.training_queue.len())
        .sum::<usize>();
    let completed = construction::completed(client);
    // age, selected kind, hp, max hp, construction, food, research kind/progress,
    // research availability, build availability entries, trained kind.
    let mut v = vec![0.0; 8 + Tech::ALL.len()];
    v[1] = -1.0;
    v[4] = 1000.0;
    v[5] = -1.0;
    v[6] = -1.0;
    if let Some(p) = player {
        v[0] = p.age as u8 as f64;
    }
    let b = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id);
    if let Some(b) = b {
        v[1] = b.kind as u8 as f64;
        v[2] = f64::from(b.hp);
        v[3] = f64::from(b.max_hp);
        v[4] = f64::from(b.progress_permille);
        v[5] = b.food_remaining.map_or(-1.0, f64::from);
        if let Some(r) = b.research {
            v[6] = r.tech as u8 as f64;
            v[7] = r.progress_ticks as f64 / (r.total_ticks as f64).max(1.0);
        }
        for (i, r) in aoe_simulation::TECH_RULES.iter().enumerate() {
            let duplicate = client.village.buildings.iter().any(|b| {
                b.player == owner(client)
                    && b.research.is_some_and(|entry| entry.tech as usize == i)
            });
            v[8 + i] =
                availability::research(b, player, owner(client), completed, duplicate, r).number();
        }
    }
    for r in &aoe_simulation::BUILDING_RULES[1..] {
        let kind = KINDS[r.kind as usize];
        v.push(f64::from(u8::from(construction::enabled(client, kind, 1))));
    }
    v.push(
        b.and_then(|b| b.training_queue.first())
            .map_or(0.0, |e| e.kind as u8 as f64),
    );
    let train = b.map_or(Availability::Unavailable, |b| {
        availability::training(
            b,
            player,
            owner(client),
            reserved,
            availability::primary_training(b, player),
        )
    });
    v.push(train.number());
    let foundations: Vec<_> = client
        .village
        .buildings
        .iter()
        .filter(|b| b.construction == ConstructionState::UnderConstruction)
        .collect();
    v.push(foundations.len() as f64);
    let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
    for b in foundations {
        let p = map::screen_position(client, super::building_center(b));
        v.extend_from_slice(&[p.x * scale, p.y * scale, f64::from(b.progress_permille)]);
    }
    for kind in aoe_protocol::UnitKind::ALL {
        let ready = b.map_or(Availability::Unavailable, |b| {
            availability::training(b, player, owner(client), reserved, kind)
        });
        v.push(ready.number());
    }
    v.push(f64::from(u8::from(
        player.is_some_and(|p| p.researched & Tech::ManAtArms.bit() != 0),
    )));
    let market = b.is_some_and(|b| {
        b.kind == aoe_protocol::BuildingKind::Market
            && b.player == owner(client)
            && b.construction == ConstructionState::Complete
    });
    v.extend(match player {
        Some(p) => building_ui::market_panel(market, p.market_prices, p.stockpile),
        None => [0.0; 15],
    });
    let reseeds = b.map_or(0, |b| b.farm_reseeds);
    let mill = b.is_some_and(|b| {
        b.kind == aoe_protocol::BuildingKind::Mill
            && b.player == owner(client)
            && b.construction == ConstructionState::Complete
    });
    let cost = aoe_simulation::BuildingKind::Farm.rules().cost.wood;
    v.extend([
        f64::from(reseeds),
        f64::from(u8::from(
            mill && reseeds < 99 && player.is_some_and(|p| p.stockpile.wood >= cost),
        )),
        f64::from(u8::from(mill && reseeds > 0)),
        f64::from(cost),
    ]);
    v.extend(super::automation::state(client));
    buildings_update(&v);
}

pub(super) const KINDS: [aoe_protocol::BuildingKind; 16] = aoe_protocol::BuildingKind::ALL;

#[inline(never)]
pub(in super::super) fn preview(client: &Client) {
    let mut v = Vec::new();
    if let Some(kind) = client.village.placement
        && let Some(at) = client.pointer.and_then(|p| construction::origin(client, p))
    {
        let tiles = construction::tiles(client, at).unwrap_or_default();
        let valid = !tiles.is_empty()
            && construction::enabled(client, kind, tiles.len() as u32)
            && tiles.iter().all(|t| construction::valid(client, kind, *t));
        v.push(f64::from(u8::from(valid)));
        let side = f64::from(kind.side());
        let scale = f64::from(client.canvas.client_width()) / f64::from(client.canvas.width());
        for t in tiles {
            for (dx, dy) in [
                (0.0, 0.0),
                (side, 0.0),
                (side, f64::from(kind.depth())),
                (0.0, f64::from(kind.depth())),
            ] {
                let p = map::screen_position(client, [f64::from(t.x) + dx, f64::from(t.y) + dy]);
                v.extend_from_slice(&[p.x * scale, p.y * scale]);
            }
        }
    }
    placement_update(&v);
}

// Numeric UI actions are wired in the DOM module that creates the buttons.
// Ownership, availability and ordinary protocol encoding stay in Rust.
#[inline(never)]
fn command(client: &mut Client, action: u32) {
    match action {
        0..=14 => construction::arm(client, KINDS[action as usize + 1]),
        33 => super::automation::toggle(client),
        34..=41 => {
            super::orders::train_kind(client, aoe_protocol::UnitKind::ALL[(action - 34) as usize])
        }
        48 => super::orders::train(client, false),
        49 => super::orders::stop(client),
        50 => {
            client.village.move_cursor = true;
            let _ = client.canvas.set_attribute("data-order-mode", "move");
        }
        51 => super::orders::train(client, true),
        _ => {
            let Some(building_id) = client.selected.building_id else {
                return;
            };
            if !client
                .village
                .buildings
                .iter()
                .any(|b| b.id == building_id && b.player == owner(client))
            {
                return;
            }
            let sequence = client.next_sequence;
            let message = match action {
                15..=28 => Message::ResearchOrder {
                    sequence,
                    building_id,
                    tech: Tech::ALL[(action - 15) as usize],
                },
                29 => Message::CancelConstructionOrder {
                    sequence,
                    building_id,
                },
                30 => Message::CancelResearch {
                    sequence,
                    building_id,
                },
                31..=32 => Message::FarmReseedOrder {
                    sequence,
                    building_id,
                    add: action == 31,
                },
                42..=44 => Message::MarketBuy {
                    sequence,
                    building_id,
                    resource: aoe_protocol::TradeResource::ALL[(action - 42) as usize],
                },
                45..=47 => Message::MarketSell {
                    sequence,
                    building_id,
                    resource: aoe_protocol::TradeResource::ALL[(action - 45) as usize],
                },
                _ => return,
            };
            send(client, message);
        }
    }
}

pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback = Closure::<dyn FnMut(u32)>::new(move |action| {
        command(&mut shared.borrow_mut(), action);
    });
    commands_bind(callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}
