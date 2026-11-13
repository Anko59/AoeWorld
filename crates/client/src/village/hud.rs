use super::{Client, building_center, owner};
use crate::playground::game::{map, position_of};
use aoe_protocol::{ResourceKind, UnitKind, UnitTask};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
use web_sys::{Element, MouseEvent};

#[wasm_bindgen(
    inline_js = "export function village_update(values, status, details) { window.aoeInterface?.numeric(values, status); window.aoeInterface?.details(details); }"
)]
extern "C" {
    fn village_update(values: &[f64], status: &str, details: &[f64]);
}

#[wasm_bindgen(
    inline_js = "export function kingdom_activity_update(v) { window.aoeActivity?.update(v); }"
)]
extern "C" {
    fn kingdom_activity_update(values: &[f64]);
}

fn flag(value: bool) -> f64 {
    f64::from(u8::from(value))
}

/// The colour the scene renders this player with, packed as 0xRRGGBB.
fn rendered_colour(client: &Client, player: aoe_core::PlayerId, owner: aoe_core::PlayerId) -> f64 {
    let [r, g, b] = aoe_rendering::player_colour(player, owner, client.team_colour).map(u32::from);
    f64::from(r << 16 | g << 8 | b)
}

fn screen(
    client: &Client,
    position: [f64; 2],
    scale: [f64; 2],
    viewport: [f64; 2],
    selected: bool,
) -> Option<[f64; 2]> {
    crate::hud_visibility::project_if_horizontal(
        client.camera,
        position,
        scale,
        viewport,
        selected,
        || {
            let p = map::screen_position(client, position);
            [p.x * scale[0], p.y * scale[1]]
        },
    )
}

pub(in super::super) fn update(client: &Client) {
    let stance = client
        .selected
        .units
        .first()
        .and_then(|id| client.units.get(id))
        .filter(|first| {
            client.selected.units.iter().all(|id| {
                client
                    .units
                    .get(id)
                    .is_some_and(|u| u.stance == first.stance)
            })
        })
        .map_or("", |u| ["0", "1", "2", "3"][u.stance as usize]);
    if client
        .canvas
        .get_attribute("data-selected-stance")
        .as_deref()
        != Some(stance)
    {
        let _ = client.canvas.set_attribute("data-selected-stance", stance);
    }
    let owner = owner(client);
    let overview = crate::playground::game::strategic::active(client);
    // Header: four stockpiles, used/cap, four worker counts, clock,
    // native-art flag, actor/queue/resource counts, and the two idle counts.
    // Actor records contain eighteen numbers; queue records contain progress,
    // total and unit kind.
    // Decode immediately in JS; never retain a view of WASM memory.
    // One geometry snapshot precedes every panel write. Later panels share it
    // instead of forcing layout after the numeric HUD has changed the DOM.
    let css_viewport = [client.canvas.client_width(), client.canvas.client_height()];
    let viewport = css_viewport.map(|size| f64::from(size.max(1)));
    let panel_scale = f64::from(css_viewport[0]) / f64::from(client.canvas.width());
    let scale = [
        viewport[0] / f64::from(client.canvas.width()),
        viewport[1] / f64::from(client.canvas.height()),
    ];
    let camera = aoe_rendering::SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let heads = if overview {
        Vec::new()
    } else {
        // Reuse the displayed pose instead of rebuilding every world actor to
        // find the few selected health-bar anchors.
        client
            .selected
            .units
            .iter()
            .filter_map(|id| client.rendered_frame.unit(*id))
            .filter_map(|u| {
                aoe_rendering::scene_unit_bounds(
                    &client.art,
                    u,
                    camera,
                    (client.last_frame / 100.0) as usize,
                )
                .map(|bounds| {
                    (
                        u.id,
                        (bounds[1] - 5.0) * f64::from(css_viewport[1])
                            / f64::from(client.canvas.height()),
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    let head = |id, y| {
        heads
            .iter()
            .find(|(key, _)| *key == id)
            .map_or(y - 42.0 * client.camera.zoom, |(_, top)| *top)
    };
    let mut values = vec![0.0; 17];
    values[15] = client.village.idle_villagers.len() as f64;
    values[16] = client.village.idle_military.len() as f64;
    values[10] = (client.village.tick / u64::from(client.config.tick_hz.max(1))) as f64;
    values[11] = flag(!client.art.village[0].is_empty());
    if let Some(player) = client.village.players.iter().find(|p| p.player == owner) {
        let s = player.stockpile;
        values[..6].copy_from_slice(&[
            f64::from(s.wood),
            f64::from(s.food),
            f64::from(s.gold),
            f64::from(s.stone),
            f64::from(player.population_used),
            f64::from(player.population_cap),
        ]);
    }
    for unit in client.units.values() {
        let selected = client.selected.units.contains(&unit.id);
        if unit.player == owner
            && matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip)
            && let Some(kind) = worker_resource(unit)
        {
            values[6 + match kind {
                ResourceKind::Wood => 0,
                ResourceKind::Food => 1,
                ResourceKind::Gold => 2,
                ResourceKind::Stone => 3,
            }] += 1.0;
        }
        // Keep global worker totals, but only serialize selected actor details
        // in strategic views. The minimap/cluster stream remains complete.
        if overview && !selected {
            continue;
        }
        let position = position_of(client, unit);
        let Some([x, y]) = screen(client, position, scale, viewport, selected) else {
            continue;
        };
        let top = head(unit.id, y);
        if !crate::hud_visibility::visible([x, y], top, viewport, selected) {
            continue;
        }
        values[12] += 1.0;
        let resource = unit.carried.map(|load| load.kind).or(unit.gathering);
        let base = crate::building_ui::content::rules().units[unit.kind as usize].combat;
        let p = client
            .village
            .players
            .iter()
            .find(|p| p.player == unit.player);
        let stats = aoe_simulation::apply_research_stats(
            base,
            p.is_some_and(|p| p.age >= aoe_protocol::Age::Feudal),
            p.map_or(0, |p| p.researched),
        );
        let loom = client
            .village
            .players
            .iter()
            .find(|p| p.player == unit.player)
            .is_some_and(|p| p.researched & 1 != 0)
            && unit.kind == UnitKind::Villager;
        values.extend_from_slice(&[
            f64::from(unit.id.0),
            match unit.kind {
                UnitKind::Scout => 0.0,
                UnitKind::Villager => 1.0,
                UnitKind::Militia => 2.0 + aoe_protocol::BuildingKind::ALL.len() as f64,
                UnitKind::FishingShip => 3.0 + aoe_protocol::BuildingKind::ALL.len() as f64,
                _ => {
                    8.0 + aoe_protocol::BuildingKind::ALL.len() as f64
                        + (unit.kind as u8 - UnitKind::Archer as u8) as f64
                }
            },
            x,
            y,
            flag(selected),
            unit.task as u8 as f64,
            resource.map_or(4.0, |kind| kind as u8 as f64),
            f64::from(unit.carried.map_or(0, |load| load.amount)),
            f64::from(unit.hp),
            f64::from(unit.max_hp),
            f64::from(stats.attack),
            f64::from(stats.melee_armour) + if loom { 1.0 } else { 0.0 },
            f64::from(stats.pierce_armour) + if loom { 2.0 } else { 0.0 },
            top,
            f64::from(if unit.kind == UnitKind::FishingShip {
                15
            } else {
                let researched = p.map_or(0, |p| p.researched);
                let capacity = crate::building_ui::content::rules().villager_capacity(researched)
                    + u16::from(
                        unit.task == UnitTask::Farming
                            && researched & aoe_protocol::Tech::HeavyPlow.bit() != 0,
                    );
                capacity.max(unit.carried.map_or(0, |load| load.amount))
            }),
            f64::from(stats.range),
            f64::from(unit.player.0),
            flag(unit.player == owner),
            rendered_colour(client, unit.player, owner),
        ]);
    }
    for building in &client.village.buildings {
        let selected = client.selected.has_building(building.id);
        if overview && !selected {
            continue;
        }
        let Some([x, y]) = screen(client, building_center(building), scale, viewport, selected)
        else {
            continue;
        };
        if !crate::hud_visibility::visible([x, y], y - 48.0, viewport, selected) {
            continue;
        }
        values[12] += 1.0;
        values.extend_from_slice(&[
            f64::from(building.id.0),
            2.0 + building.kind as u8 as f64,
            x,
            y,
            flag(selected),
            if building.training_queue.is_empty() {
                5.0
            } else {
                6.0
            },
            4.0,
            0.0,
            f64::from(building.hp),
            f64::from(building.max_hp),
            0.0,
            0.0,
            0.0,
            y,
            // Garrisoned occupants (the unit-only `capacity` slot): the map flag.
            building.garrison.len() as f64,
            0.0,
            f64::from(building.player.0),
            flag(building.player == owner),
            rendered_colour(client, building.player, owner),
        ]);
    }
    for animal in &client.village.animals {
        let selected = client.selected.units.contains(&animal.id);
        if overview && !selected {
            continue;
        }
        let Some([x, y]) = screen(
            client,
            animal.position.as_tiles(),
            scale,
            viewport,
            selected,
        ) else {
            continue;
        };
        let top = head(animal.id, y);
        if !crate::hud_visibility::visible([x, y], top, viewport, selected) {
            continue;
        }
        values[12] += 1.0;
        let (kind, hp, attack) = match animal.kind {
            aoe_protocol::AnimalKind::Sheep => {
                (4.0 + aoe_protocol::BuildingKind::ALL.len() as f64, 7, 0)
            }
            aoe_protocol::AnimalKind::Deer => {
                (5.0 + aoe_protocol::BuildingKind::ALL.len() as f64, 5, 0)
            }
            aoe_protocol::AnimalKind::Boar => {
                (6.0 + aoe_protocol::BuildingKind::ALL.len() as f64, 75, 7)
            }
            aoe_protocol::AnimalKind::Wolf => {
                (7.0 + aoe_protocol::BuildingKind::ALL.len() as f64, 25, 3)
            }
        };
        values.extend_from_slice(&[
            f64::from(animal.id.0),
            kind,
            x,
            y,
            flag(selected),
            13.0 + animal.task as u8 as f64,
            0.0,
            f64::from(animal.food_remaining),
            f64::from(animal.hp),
            f64::from(hp),
            f64::from(attack),
            0.0,
            0.0,
            top,
            0.0,
            0.0,
            animal.owner.map_or(-1.0, |p| f64::from(p.0)),
            flag(animal.owner == Some(owner) && animal.hp > 0),
            animal
                .owner
                .map_or(0.0, |p| rendered_colour(client, p, owner)),
        ]);
    }
    if let Some(building) = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id && b.player == owner)
    {
        values[13] = building.training_queue.len() as f64;
        for entry in &building.training_queue {
            values.extend_from_slice(&[
                entry.progress_ticks as f64,
                entry.total_ticks as f64,
                entry.kind as u8 as f64,
            ]);
        }
    }
    let details = super::presentation::values(client);
    village_update(&values, map::feedback::connection_label(client), &details);
    super::building_panel::update(client, panel_scale);
    let activity = crate::kingdom_activity::summarize(&client.village.buildings, owner);
    let mut rows = vec![activity.more as f64];
    for entry in activity.entries {
        rows.extend_from_slice(&[
            flag(entry.research),
            f64::from(entry.kind),
            f64::from(entry.count),
            f64::from(entry.progress),
            f64::from(entry.building.0),
        ]);
    }
    kingdom_activity_update(&rows);
    super::steward::present(client, panel_scale);
    crate::playground::game::strategic::update(client);
}

pub(in super::super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    for (id, military) in [("idle-villagers", false), ("idle-military", true)] {
        let finder = shared.clone();
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
            super::selection::idle(&mut finder.borrow_mut(), military);
        });
        shared
            .borrow()
            .document
            .get_element_by_id(id)
            .ok_or("idle finder button missing")?
            .add_event_listener_with_callback(
                "click",
                wasm_bindgen::JsCast::unchecked_ref(callback.as_ref()),
            )?;
        callback.forget();
    }
    // One delegated listener serves every portrait the panel renders.
    let panel = shared.clone();
    let portraits =
        wasm_bindgen::closure::Closure::<dyn FnMut(MouseEvent)>::new(move |event: MouseEvent| {
            let token = event
                .target()
                .and_then(|target| target.dyn_into::<Element>().ok())
                .and_then(|element| element.closest(".unit-token").ok().flatten());
            let Some(id) = token
                .and_then(|token| token.get_attribute("data-unit-id"))
                .and_then(|id| id.parse::<u32>().ok())
            else {
                return;
            };
            super::selection::portrait(
                &mut panel.borrow_mut(),
                aoe_core::EntityId(id),
                event.shift_key(),
                event.ctrl_key(),
            );
        });
    shared
        .borrow()
        .document
        .get_element_by_id("selected-icons")
        .ok_or("selected units panel missing")?
        .add_event_listener_with_callback(
            "click",
            wasm_bindgen::JsCast::unchecked_ref(portraits.as_ref()),
        )?;
    portraits.forget();
    super::building_panel::install(shared)
}

/// Villagers and Fishing Ships working a resource, including farming.
/// The resource a worker's job gathers, as AoE2's top-bar counts it: every
/// stage of the job (walking to the resource, working, carrying it back or
/// waiting for a drop-off) and every food job (forager, hunter, shepherd,
/// fisherman, farmer). Idle, building, fighting and dead workers count for
/// nothing, whatever they carry.
pub(in super::super) fn worker_resource(
    unit: &aoe_protocol::GameplayUnitState,
) -> Option<ResourceKind> {
    if unit.hp == 0
        || matches!(
            unit.task,
            UnitTask::Idle
                | UnitTask::Building
                | UnitTask::Seeding
                | UnitTask::Attacking
                | UnitTask::Dying
        )
    {
        return None;
    }
    unit.gathering
        .or(unit.hunt_target.map(|_| ResourceKind::Food))
}
