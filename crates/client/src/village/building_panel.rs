//! Bounded numeric building presentation; no DOM rules duplicate the simulation.
use super::{Client, construction, orders::send, owner};
use crate::{building_ui, playground::game::map};
use aoe_protocol::{ConstructionState, GameplayClientMessage as Message, Tech};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

#[wasm_bindgen(
    inline_js = "export function buildings_update(v) { window.aoeBuildings?.update(v); } export function placement_update(v) { window.aoeBuildings?.preview(v); }"
)]
extern "C" {
    fn buildings_update(v: &[f64]);
    fn placement_update(v: &[f64]);
}

#[inline(never)]
pub(super) fn update(client: &Client) {
    let player = client
        .village
        .players
        .iter()
        .find(|p| p.player == owner(client));
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
            let requirement = match r.requires {
                aoe_simulation::TechRequirement::None => true,
                aoe_simulation::TechRequirement::Age(age) => {
                    age == aoe_simulation::Age::Dark
                        || player.is_some_and(|p| p.age == aoe_protocol::Age::Feudal)
                }
                aoe_simulation::TechRequirement::BuildingKinds { count, kinds } => {
                    kinds
                        .iter()
                        .filter(|k| construction::completed(client) & (1 << **k as u8) != 0)
                        .count()
                        >= count
                }
            };
            let enabled = r.building as u8 == b.kind as u8
                && b.player == owner(client)
                && b.construction == ConstructionState::Complete
                && b.training_queue.is_empty()
                && b.research.is_none()
                && requirement
                && player.is_some_and(|p| {
                    p.researched & (1 << i) == 0
                        && building_ui::affordable(p.stockpile, building_ui::stock(r.cost))
                })
                && !client.village.buildings.iter().any(|b| {
                    b.player == owner(client) && b.research.is_some_and(|r| r.tech as usize == i)
                });
            v[8 + i] = f64::from(u8::from(enabled));
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
    let train_enabled = b.is_some_and(|b| {
        let rules = building_ui::rules(b.kind);
        let cost = if b.kind == aoe_protocol::BuildingKind::TownCenter {
            aoe_protocol::Stockpile {
                food: aoe_simulation::VILLAGE_RULES.villager_food_cost,
                ..Default::default()
            }
        } else {
            let c = aoe_simulation::GameWorld::unit_training(
                if b.kind == aoe_protocol::BuildingKind::Dock {
                    aoe_simulation::UnitKind::FishingShip
                } else {
                    aoe_simulation::UnitKind::Militia
                },
            )
            .map(|t| t.cost)
            .unwrap_or_default();
            aoe_protocol::Stockpile {
                food: c.food,
                wood: c.wood,
                gold: c.gold,
                stone: c.stone,
            }
        };
        let reserved = client
            .village
            .buildings
            .iter()
            .filter(|v| v.player == owner(client))
            .map(|v| v.training_queue.len())
            .sum::<usize>();
        b.player == owner(client)
            && b.construction == ConstructionState::Complete
            && !rules.trainable.is_empty()
            && b.research.is_none()
            && b.training_queue.len() < aoe_protocol::MAX_TRAINING_QUEUE
            && player.is_some_and(|p| {
                building_ui::affordable(p.stockpile, cost)
                    && usize::from(p.population_used) + reserved < usize::from(p.population_cap)
            })
    });
    v.push(f64::from(u8::from(train_enabled)));
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
        let ready = b.is_some_and(|b| {
            let native = aoe_simulation::UNIT_COMBAT[kind as usize].kind;
            let r = building_ui::rules(b.kind);
            let cost = aoe_simulation::GameWorld::unit_training(native).map(|t| t.cost);
            b.player == owner(client)
                && b.progress_permille == 1000
                && b.research.is_none()
                && b.training_queue.len() < aoe_protocol::MAX_TRAINING_QUEUE
                && r.trainable.contains(&native)
                && player.is_some_and(|p| {
                    p.age == aoe_protocol::Age::Feudal
                        && (kind != aoe_protocol::UnitKind::ManAtArms
                            || p.researched & Tech::ManAtArms.bit() != 0)
                        && cost.is_some_and(|c| {
                            building_ui::affordable(
                                p.stockpile,
                                aoe_protocol::Stockpile {
                                    food: c.food,
                                    wood: c.wood,
                                    gold: c.gold,
                                    stone: c.stone,
                                },
                            )
                        })
                        && usize::from(p.population_used)
                            + client
                                .village
                                .buildings
                                .iter()
                                .filter(|b| b.player == p.player)
                                .map(|b| b.training_queue.len())
                                .sum::<usize>()
                            < usize::from(p.population_cap)
                })
        });
        v.push(f64::from(u8::from(ready)));
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

#[inline(never)]
pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    for (i, id) in [
        "build-house",
        "build-lumber-camp",
        "build-mining-camp",
        "build-mill",
        "build-farm",
        "build-barracks",
        "build-palisade-wall",
        "build-outpost",
        "build-dock",
        "build-archery-range",
        "build-stable",
        "build-blacksmith",
        "build-market",
        "build-watch-tower",
        "build-palisade-gate",
        "research-loom",
        "research-town-watch",
        "research-feudal-age",
        "research-double-bit-axe",
        "research-horse-collar",
        "research-gold-mining",
        "research-stone-mining",
        "research-wheelbarrow",
        "research-man-at-arms",
        "research-forging",
        "research-scale-mail-armor",
        "research-scale-barding-armor",
        "research-fletching",
        "research-padded-archer-armor",
        "cancel-construction",
        "cancel-research",
    ]
    .iter()
    .enumerate()
    {
        let Some(button) = shared.borrow().document.get_element_by_id(id) else {
            continue;
        };
        let target = shared.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let mut client = target.borrow_mut();
            if i < 15 {
                construction::arm(&mut client, KINDS[i + 1]);
                return;
            }
            let Some(building_id) = client.selected.building_id else {
                return;
            };
            let sequence = client.next_sequence;
            let message = match i {
                15..=28 => Message::ResearchOrder {
                    sequence,
                    building_id,
                    tech: Tech::ALL[i - 15],
                },
                29 => Message::CancelConstructionOrder {
                    sequence,
                    building_id,
                },
                _ => Message::CancelResearch {
                    sequence,
                    building_id,
                },
            };
            send(&mut client, message);
        });
        button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
        callback.forget();
    }
    for (id, kind) in [
        ("train-archer", aoe_protocol::UnitKind::Archer),
        ("train-skirmisher", aoe_protocol::UnitKind::Skirmisher),
        ("train-spearman", aoe_protocol::UnitKind::Spearman),
        ("train-scout", aoe_protocol::UnitKind::Scout),
    ] {
        let Some(button) = shared.borrow().document.get_element_by_id(id) else {
            continue;
        };
        let target = shared.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            super::orders::train_kind(&mut target.borrow_mut(), kind);
        });
        button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
        callback.forget();
    }
    for (i, id) in [
        "market-buy-wood",
        "market-buy-food",
        "market-buy-stone",
        "market-sell-wood",
        "market-sell-food",
        "market-sell-stone",
    ]
    .iter()
    .enumerate()
    {
        let Some(button) = shared.borrow().document.get_element_by_id(id) else {
            continue;
        };
        let target = shared.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let mut client = target.borrow_mut();
            let Some(building_id) = client.selected.building_id else {
                return;
            };
            let sequence = client.next_sequence;
            let resource = aoe_protocol::TradeResource::ALL[i % 3];
            let message = if i < 3 {
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
            };
            send(&mut client, message);
        });
        button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
        callback.forget();
    }
    Ok(())
}
