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
    inline_js = "export function buildings_update(v) { window.aoeBuildings?.update(v); } export function placement_update(v) { window.aoeBuildings?.preview(v); } export function buildings_bind(callback) { window.aoeBuildings.bind(callback); }"
)]
extern "C" {
    fn buildings_update(v: &[f64]);
    fn placement_update(v: &[f64]);
    #[wasm_bindgen(catch)]
    fn buildings_bind(callback: &js_sys::Function) -> Result<(), JsValue>;
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
        for (i, r) in building_ui::content::rules().techs.iter().enumerate() {
            let duplicate = client.village.buildings.iter().any(|b| {
                b.player == owner(client)
                    && b.research.is_some_and(|entry| entry.tech as usize == i)
            });
            v[8 + i] =
                availability::research(b, player, owner(client), completed, duplicate, r).number();
        }
    }
    for r in &building_ui::content::rules().buildings[1..] {
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
    v.push(b.map_or(aoe_protocol::UnitKind::Militia, |b| {
        availability::primary_training(b, player)
    }) as u8 as f64);
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
    let cost = building_ui::rules(aoe_protocol::BuildingKind::Farm)
        .cost
        .wood;
    v.extend([
        f64::from(reseeds),
        f64::from(u8::from(
            mill && reseeds < 99 && player.is_some_and(|p| p.stockpile.wood >= cost),
        )),
        f64::from(u8::from(mill && reseeds > 0)),
        f64::from(cost),
    ]);
    v.extend(super::automation::state(client));
    super::garrison::present(client);
    v.extend(super::navy::state(client));
    buildings_update(&v);
}

pub(super) const KINDS: [aoe_protocol::BuildingKind; 21] = aoe_protocol::BuildingKind::ALL;

#[inline(never)]
pub(in super::super) fn preview(client: &Client, units: &mut Vec<aoe_rendering::SceneUnit>) {
    let mut state = Vec::new();
    if let Some(kind) = client.village.placement
        && let Some(at) = client.pointer.and_then(|p| construction::origin(client, p))
    {
        let tiles = construction::tiles(client, at).unwrap_or_default();
        let valid = construction::span_valid(client, kind, &tiles);
        state.push(f64::from(u8::from(valid)));
        let age = client
            .village
            .players
            .iter()
            .find(|p| p.player == owner(client))
            .map_or(aoe_protocol::Age::Dark, |p| p.age);
        for (index, t) in tiles.iter().enumerate() {
            let position = [
                f64::from(t.x) + f64::from(kind.side()) / 2.0,
                f64::from(t.y) + f64::from(kind.depth()) / 2.0,
            ];
            units.push(aoe_rendering::SceneUnit {
                kind: aoe_rendering::SceneUnitKind::building(building_ui::age_sprite_slot(
                    kind, 1000, false, age,
                ) as u8),
                id: aoe_core::EntityId((1 << 31) + index as u32),
                position,
                visual: aoe_rendering::SceneUnitVisual::new(false)
                    .with_player(
                        Some(owner(client)),
                        owner(client),
                        client.team_colour,
                        false,
                    )
                    .with_ghost(valid),
                facing: if kind == aoe_protocol::BuildingKind::PalisadeWall {
                    building_ui::wall_frame(*t, |p| tiles.contains(&p))
                } else {
                    0
                },
                elevation_meters: map::elevation_at_world(client, position),
            });
        }
    }
    placement_update(&state);
}

// Numeric UI actions are wired in the DOM module that creates the buttons.
// Ownership, availability and ordinary protocol encoding stay in Rust.
#[inline(never)]
pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    super::garrison::install(shared.clone())?;
    // The DOM owns button identities; one bounded callback dispatches ordinary
    // orders. Keep the research/build/unit tables out of a second string list.
    let callback = Closure::<dyn FnMut(u32, u32)>::new(move |action, code| {
        let mut client = shared.borrow_mut();
        if action == 5 {
            super::orders::formation(&mut client, code);
            return;
        }
        if action == 6 {
            client.village.garrison_cursor = false;
            client.village.unload_cursor = false;
            client.village.attack_move = true;
            client.village.move_cursor = true;
            let _ = client
                .canvas
                .set_attribute("data-order-mode", "attack-move");
            return;
        }
        if action == 4 {
            super::navy::arm_unload(&mut client);
            return;
        }
        if action == 3 {
            match code {
                0 => super::orders::train(&mut client, false),
                1 => super::orders::stop(&mut client),
                2 => {
                    client.village.garrison_cursor = false;
                    client.village.unload_cursor = false;
                    client.village.attack_move = false;
                    client.village.move_cursor = true;
                    let _ = client.canvas.set_attribute("data-order-mode", "move");
                }
                3 => super::orders::train(&mut client, true),
                _ => (),
            }
            return;
        }
        let code = code as usize;
        let build_count = KINDS.len() - 1;
        let research_end = build_count + Tech::ALL.len();
        if action == 0 && code == research_end + 4 {
            super::automation::toggle(&mut client);
            return;
        }
        if action == 0 && code < build_count {
            construction::arm(&mut client, KINDS[code + 1]);
            return;
        }
        if action == 1 {
            if let Some(&kind) = aoe_protocol::UnitKind::ALL.get(code) {
                super::orders::train_kind(&mut client, kind);
            }
            return;
        }
        let Some(building_id) = client.selected.building_id else {
            return;
        };
        if !client
            .village
            .buildings
            .iter()
            .any(|b| b.id == building_id && b.player == owner(&client))
        {
            return;
        }
        let sequence = client.next_sequence;
        let message = if action == 0 {
            match code {
                i if i < research_end => Message::ResearchOrder {
                    sequence,
                    building_id,
                    tech: Tech::ALL[i - build_count],
                },
                i if i == research_end => Message::CancelConstructionOrder {
                    sequence,
                    building_id,
                },
                i if i == research_end + 1 => Message::CancelResearch {
                    sequence,
                    building_id,
                },
                i if i == research_end + 2 || i == research_end + 3 => Message::FarmReseedOrder {
                    sequence,
                    building_id,
                    add: i == research_end + 2,
                },
                _ => return,
            }
        } else if action == 2 && code < 6 {
            let resource = aoe_protocol::TradeResource::ALL[code % 3];
            if code < 3 {
                Message::MarketBuy {
                    sequence,
                    building_id,
                    resource,
                }
            } else {
                Message::MarketSell {
                    sequence,
                    building_id,
                    resource,
                }
            }
        } else {
            return;
        };
        send(&mut client, message);
    });
    buildings_bind(callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}
