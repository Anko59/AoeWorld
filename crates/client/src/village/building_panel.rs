//! Bounded numeric building presentation; no DOM rules duplicate the simulation.
use super::{Client, construction, orders::send, owner};
use crate::{
    building_ui::{self, availability},
    playground::game::map,
};
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayClientMessage as Message, Tech, UnitKind,
};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

mod catalog;

#[wasm_bindgen(
    inline_js = "export function buildings_update(v, tower) { window.aoeBuildings?.update(v, tower); } export function placement_update(v) { window.aoeBuildings?.preview(v); } export function age_progress_update(age, fraction) { window.aoeBuildings?.ageProgress(age, fraction); } export function buildings_bind(callback, catalog) { window.aoeBuildings.catalog(catalog); window.aoeBuildings.bind(callback); }"
)]
extern "C" {
    fn buildings_update(v: &[f64], tower: &[f64]);
    fn placement_update(v: &[f64]);
    fn age_progress_update(age: f64, fraction: f64);
    #[wasm_bindgen(catch)]
    fn buildings_bind(callback: &js_sys::Function, catalog: &[f64]) -> Result<(), JsValue>;
}

/// Target age (1 Feudal, 2 Castle, 3 Imperial) and completed fraction of the
/// owner's age-up research, found on whichever building carries it, so the top
/// bar needs no selection. `None` while no age is being researched.
pub(super) fn age_research_progress(
    buildings: &[aoe_protocol::GameplayBuildingState],
    owner: aoe_core::PlayerId,
) -> Option<(u8, f64)> {
    buildings
        .iter()
        .filter(|b| b.player == owner)
        .filter_map(|b| b.research)
        .find_map(|r| {
            let age = match r.tech {
                Tech::FeudalAge => 1,
                Tech::CastleAge => 2,
                Tech::ImperialAge => 3,
                _ => return None,
            };
            Some((
                age,
                (r.progress_ticks as f64 / r.total_ticks.max(1) as f64).clamp(0.0, 1.0),
            ))
        })
}

#[inline(never)]
pub(super) fn update(client: &Client, scale: f64) {
    let (age, fraction) =
        age_research_progress(&client.village.buildings, owner(client)).unwrap_or((0, 0.0));
    age_progress_update(f64::from(age), fraction);
    let player = client
        .village
        .players
        .iter()
        .find(|p| p.player == owner(client));
    let completed = construction::completed(client);
    // age, selected kind, hp, max hp, construction, food, research kind/progress,
    // research availability, build availability and viewport foundations.
    let mut v = vec![0.0; 8 + Tech::ALL.len()];
    v[1] = -1.0;
    v[4] = 1000.0;
    v[5] = -1.0;
    v[6] = -1.0;
    if let Some(p) = player {
        v[0] = p.age as u8 as f64;
    }
    let b = super::production::selected(client);
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
            if r.building != b.kind as u8 {
                continue;
            }
            let duplicate = client.village.buildings.iter().any(|b| {
                b.player == owner(client)
                    && b.research.is_some_and(|entry| entry.tech as usize == i)
            });
            v[8 + i] =
                availability::research(b, player, owner(client), completed, duplicate, r).number();
        }
    }
    let builders = player.is_some() && construction::allows_land_placement(client);
    let ships = player.is_some() && !construction::ship_builders(client).is_empty();
    let build_menu = availability::BuildMenu::new(player, completed, builders, ships);
    for kind in KINDS
        .iter()
        .copied()
        .skip(1)
        .chain(std::iter::once(KINDS[0]))
    {
        v.push(f64::from(u8::from(build_menu.enabled(kind, 1))));
    }
    let viewport = client.camera.viewport.map(|size| size * scale);
    // No bar for a foundation nobody has started (a row of twenty planned
    // buildings would be a wall of bars), and none in the far view, which
    // draws foundations as plain markers.
    let far = crate::playground::game::strategic::active(client);
    let foundations: Vec<_> = client
        .village
        .buildings
        .iter()
        .filter(|b| {
            !far && b.construction == ConstructionState::UnderConstruction
                && b.progress_permille > 0
        })
        .filter_map(|b| {
            let p = map::screen_position(client, super::building_center(b));
            let p = [p.x * scale, p.y * scale];
            // The CSS bar is 70px wide; the shared gutter also keeps its
            // raised text when only that text overlaps a viewport edge.
            crate::hud_visibility::visible(p, p[1] - 24.0, viewport, false)
                .then_some((p, b.progress_permille))
        })
        .collect();
    v.push(foundations.len() as f64);
    for (p, progress) in foundations {
        v.extend_from_slice(&[p[0], p[1], f64::from(progress)]);
    }
    if b.is_some_and(|b| b.player == owner(client)) {
        v.extend(UnitKind::ALL.map(|kind| super::production::availability(client, kind).number()));
    } else {
        v.resize(v.len() + UnitKind::ALL.len(), 0.0);
    }
    v.push(
        b.and_then(|b| availability::primary_training(b, player))
            .map_or(-1.0, |kind| kind as u8 as f64),
    );
    let market = b.is_some_and(|b| {
        b.kind == aoe_protocol::BuildingKind::Market
            && b.player == owner(client)
            && b.construction == ConstructionState::Complete
    });
    v.extend(match player {
        Some(p) => building_ui::market_panel(market, p.market_prices, p.stockpile, p.researched),
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
    v.extend(super::navy::state(client, scale));
    v.push(f64::from(u8::from(
        b.is_some_and(|b| !b.training_queue.is_empty())
            && player.is_some_and(|p| p.population_used >= p.population_cap),
    )));
    let tower = b.and_then(|b| building_ui::tower_base_defence(b.kind));
    buildings_update(&v, tower.as_ref().map_or(&[][..], |stats| stats.as_slice()));
}

/// Far-view research and bulk trade name their building instead of using the
/// selection. Ownership and completion are checked here and again by the server.
#[inline(never)]
fn far_commerce_order(client: &mut Client, action: u32, code: usize, target: u32, lots: u32) {
    let building_id = aoe_core::EntityId(target);
    let usable = |kind: Option<aoe_protocol::BuildingKind>| {
        client.village.buildings.iter().any(|b| {
            b.id == building_id
                && b.player == owner(client)
                && b.construction == ConstructionState::Complete
                && kind.is_none_or(|kind| b.kind == kind)
        })
    };
    let sequence = client.next_sequence;
    let message = if action == 12 {
        let Some(&tech) = Tech::ALL.get(code) else {
            return;
        };
        if !usable(None) {
            return;
        }
        Message::ResearchOrder {
            sequence,
            building_id,
            tech,
        }
    } else {
        if code >= 6
            || !(1..=aoe_protocol::MARKET_MAX_LOTS).contains(&lots)
            || !usable(Some(aoe_protocol::BuildingKind::Market))
        {
            return;
        }
        Message::MarketTradeOrder {
            sequence,
            building_id,
            resource: aoe_protocol::TradeResource::ALL[code % 3],
            buy: code < 3,
            lots,
        }
    };
    send(client, message);
}

pub(super) const KINDS: &[BuildingKind] = BuildingKind::ALL
    .split_at(BuildingKind::GuardTower as usize)
    .0;

#[inline(never)]
pub(in super::super) fn preview(client: &Client, units: &mut Vec<aoe_rendering::SceneUnit>) {
    let mut state = Vec::new();
    if let Some(kind) = client.village.placement
        && let Some(at) = client.pointer.and_then(|p| construction::origin(client, p))
    {
        let tiles = construction::tiles(client, at).unwrap_or_default();
        let valid = construction::span_valid(client, kind, &tiles);
        state.push(f64::from(u8::from(valid)));
        let bridge = kind == aoe_protocol::BuildingKind::Bridge;
        let count = if bridge {
            tiles.len().saturating_sub(2)
        } else {
            tiles.len()
        };
        let geometry = !bridge
            || tiles
                .first()
                .zip(tiles.last())
                .is_some_and(|(&a, &b)| aoe_core::BridgeSpan::new(a, b).is_some());
        let available = geometry && construction::enabled(client, kind, count as u32);
        let invalid = (0..tiles.len())
            .filter(|i| !available || !construction::tile_valid(client, kind, &tiles, *i))
            .count();
        let reason = if !bridge || valid {
            0
        } else if tiles.len() < 2 {
            1
        } else if !geometry {
            2
        } else if tiles.first().is_some_and(|tile| {
            map::terrain_tile(client, tile.x, tile.y)
                .is_some_and(|ground| ground.water != aoe_map::WaterKind::None)
        }) {
            3
        } else if tiles.last().is_some_and(|tile| {
            map::terrain_tile(client, tile.x, tile.y)
                .is_some_and(|ground| ground.water != aoe_map::WaterKind::None)
        }) {
            4
        } else if !available {
            5
        } else {
            6
        };
        state.extend([tiles.len() as f64, invalid as f64, f64::from(reason)]);
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
                kind: if bridge {
                    building_ui::bridge_piece(&tiles, index)
                } else {
                    aoe_rendering::SceneUnitKind::building(building_ui::age_sprite_slot(
                        kind, 1000, false, age,
                    ) as u8)
                },
                id: aoe_core::EntityId((1 << 31) + index as u32),
                position,
                visual: aoe_rendering::SceneUnitVisual::new(false)
                    .with_player(
                        Some(owner(client)),
                        owner(client),
                        client.team_colour,
                        false,
                    )
                    .with_ghost(available && construction::tile_valid(client, kind, &tiles, index)),
                facing: if kind.is_wall() {
                    building_ui::wall_frame(*t, |p| tiles.contains(&p))
                } else {
                    0
                },
                elevation_meters: map::elevation_at_world(client, position) as f32,
            });
        }
    }
    placement_update(&state);
}

// All menu controls, portraits and idle buttons enter through this callback.
#[inline(never)]
pub(in super::super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback =
        Closure::<dyn FnMut(u32, u32, u32, u32)>::new(move |action, code, target, lots| {
            menu_order(&mut shared.borrow_mut(), action, code, target, lots);
        });
    buildings_bind(callback.as_ref().unchecked_ref(), &catalog::values())?;
    callback.forget();
    Ok(())
}

fn menu_order(client: &mut Client, action: u32, code: u32, target: u32, lots: u32) {
    let build_count = KINDS.len();
    let research_end = build_count + Tech::ALL.len();
    match (action, code) {
        (0, code) if (code as usize) < build_count => {
            construction::arm(client, KINDS[(code as usize + 1) % build_count]);
        }
        (0, code) if code as usize == research_end + 4 => super::automation::toggle(client),
        (0 | 2, _) => selected_building_order(client, action, code as usize),
        (1, _) => {
            if let Some(&kind) = UnitKind::ALL.get(code as usize) {
                super::production::train(client, kind, lots == 5);
            }
        }
        (3, 1) => super::orders::stop(client),
        (3, 2) => arm_move(client, "move"),
        (3, 3) => super::orders::cancel_training(client),
        (3, 4) => super::orders::delete(client),
        (4, _) => super::navy::arm_unload(client),
        (5, _) => super::orders::formation(client, code),
        (6, 0) => {
            if super::selection::owned_units(client)
                .any(|unit| super::orders::can_attack_move(unit.kind))
            {
                construction::cancel(client);
                arm_move(client, "attack-move");
            }
        }
        (6, 1) => {
            if super::selection::owned_units(client)
                .any(|unit| super::orders::can_choose_stance(unit.kind))
            {
                construction::cancel(client);
                arm_move(client, "patrol");
            }
        }
        (7, _) => super::orders::stance(client, code),
        (8, _) => arm_move(client, "general-move"),
        (9 | 10, _) => super::steward::general_command(client, action, code),
        (11, _) => construction::cancel(client),
        (12 | 13, _) => far_commerce_order(client, action, code as usize, target, lots),
        (19, _) => super::production::here::order(client, code as usize, lots),
        (14, _) => {
            if let Some((id, position)) = client
                .village
                .buildings
                .iter()
                .find(|b| b.id.0 == target && b.player == owner(client) && b.hp > 0)
                .map(|b| (b.id, super::building_center(b)))
            {
                client.selected.building(id);
                map::center_on_world(client, position);
            }
        }
        (15, _) => super::garrison::menu_action(client, code, aoe_core::EntityId(target)),
        (16, _) => super::selection::portrait(
            client,
            aoe_core::EntityId(target),
            code & 1 != 0,
            code & 2 != 0,
        ),
        (17, _) => super::selection::idle(client, code == 1),
        (18, 0..=4) => super::steward::menu_order(client, code),
        _ => (),
    }
}

fn arm_move(client: &mut Client, mode: &str) {
    client.village.clear_move_cursor();
    client.village.garrison_cursor = false;
    client.village.unload_cursor = false;
    client.village.attack_move = mode == "attack-move";
    client.village.patrol_move = mode == "patrol";
    client.village.general_only = mode == "general-move";
    client.village.move_cursor = true;
    let _ = client.canvas.set_attribute("data-order-mode", mode);
}

fn selected_building_order(client: &mut Client, action: u32, code: usize) {
    let Some(building_id) = super::production::selected(client)
        .filter(|building| building.player == owner(client))
        .map(|building| building.id)
    else {
        return;
    };
    let sequence = client.next_sequence;
    let build_count = KINDS.len();
    let research_end = build_count + Tech::ALL.len();
    let message = match (action, code) {
        (0, i) if i < research_end => Message::ResearchOrder {
            sequence,
            building_id,
            tech: Tech::ALL[i - build_count],
        },
        (0, i) if i == research_end => Message::CancelConstructionOrder {
            sequence,
            building_id,
        },
        (0, i) if i == research_end + 1 => Message::CancelResearch {
            sequence,
            building_id,
        },
        (0, i) if i == research_end + 2 || i == research_end + 3 => Message::FarmReseedOrder {
            sequence,
            building_id,
            add: i == research_end + 2,
        },
        (2, 0..=2) => Message::MarketBuy {
            sequence,
            building_id,
            resource: aoe_protocol::TradeResource::ALL[code],
        },
        (2, 3..=5) => Message::MarketSell {
            sequence,
            building_id,
            resource: aoe_protocol::TradeResource::ALL[code - 3],
        },
        _ => return,
    };
    send(client, message);
}
