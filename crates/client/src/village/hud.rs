use super::{Client, building_center, owner};
use crate::playground::game::{map, position_at};
use aoe_protocol::{ResourceKind, UnitKind, UnitTask};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(
    inline_js = "export function village_update(values, status, details) { window.aoeInterface?.numeric(values, status); window.aoeInterface?.details(details); }"
)]
extern "C" {
    fn village_update(values: &[f64], status: &str, details: &[f64]);
}

fn flag(value: bool) -> f64 {
    f64::from(u8::from(value))
}

fn screen(client: &Client, position: [f64; 2]) -> [f64; 2] {
    let p = map::screen_position(client, position);
    [
        p.x * f64::from(client.canvas.client_width()) / f64::from(client.canvas.width()),
        p.y * f64::from(client.canvas.client_height()) / f64::from(client.canvas.height()),
    ]
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
    let _ = client.canvas.set_attribute("data-selected-stance", stance);
    let owner = owner(client);
    let overview = crate::playground::game::strategic::active(client);
    // Header: four stockpiles, population, four worker counts, clock,
    // native-art flag, actor/queue/resource counts, and the two idle counts.
    // Actor records contain eighteen numbers; queue records contain progress,
    // total and unit kind.
    // Decode immediately in JS; never retain a view of WASM memory.
    let scene = map::scene::prepare(client);
    let camera = aoe_rendering::SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let heads = if overview {
        Vec::new()
    } else {
        map::scene::units(client, &scene)
            .into_iter()
            .filter(|u| u.visual.selected())
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
                        (bounds[1] - 5.0) * f64::from(client.canvas.client_height())
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
    let mut values = vec![0.0; 16];
    values[14] = client.village.idle_villagers.len() as f64;
    values[15] = client.village.idle_military.len() as f64;
    values[9] = (client.village.tick / u64::from(client.config.tick_hz.max(1))) as f64;
    values[10] = flag(!client.art.village[0].is_empty());
    if let Some(player) = client.village.players.iter().find(|p| p.player == owner) {
        let s = player.stockpile;
        values[..5].copy_from_slice(&[
            f64::from(s.wood),
            f64::from(s.food),
            f64::from(s.gold),
            f64::from(s.stone),
            f64::from(player.population_used),
        ]);
    }
    for unit in client.units.values() {
        let selected = client.selected.units.contains(&unit.id);
        if unit.player == owner
            && matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip)
            && let Some(kind) = worker_resource(unit)
        {
            values[5 + match kind {
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
        values[11] += 1.0;
        let position = position_at(client, unit.id);
        let [x, y] = screen(client, position);
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
            head(unit.id, y),
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
        ]);
    }
    for building in &client.village.buildings {
        if overview && !client.selected.has_building(building.id) {
            continue;
        }
        values[11] += 1.0;
        let [x, y] = screen(client, building_center(building));
        values.extend_from_slice(&[
            f64::from(building.id.0),
            2.0 + building.kind as u8 as f64,
            x,
            y,
            flag(client.selected.has_building(building.id)),
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
        ]);
    }
    for animal in &client.village.animals {
        if overview && !client.selected.units.contains(&animal.id) {
            continue;
        }
        values[11] += 1.0;
        let [x, y] = screen(client, animal.position.as_tiles());
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
            flag(client.selected.units.contains(&animal.id)),
            12.0 + animal.task as u8 as f64,
            0.0,
            f64::from(animal.food_remaining),
            f64::from(animal.hp),
            f64::from(hp),
            f64::from(attack),
            0.0,
            0.0,
            head(animal.id, y),
            0.0,
            0.0,
            animal.owner.map_or(-1.0, |p| f64::from(p.0)),
            flag(animal.owner == Some(owner) && animal.hp > 0),
        ]);
    }
    if let Some(building) = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id && b.player == owner)
    {
        values[12] = building.training_queue.len() as f64;
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
    super::building_panel::update(client);
    super::vassal::present(client);
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
    super::building_panel::install(shared)
}

/// Villagers and Fishing Ships working a resource, including farming.
/// The resource a worker's job gathers, as AoE2's top-bar counts it: every
/// stage of the job (walking to the resource, working, carrying it back or
/// waiting for a drop-off) and every food job (forager, hunter, shepherd,
/// fisherman, farmer). Idle, building, fighting and dead workers count for
/// nothing, whatever they carry.
pub(super) fn worker_resource(unit: &aoe_protocol::GameplayUnitState) -> Option<ResourceKind> {
    if unit.hp == 0
        || matches!(
            unit.task,
            UnitTask::Idle | UnitTask::Building | UnitTask::Attacking | UnitTask::Dying
        )
    {
        return None;
    }
    unit.gathering
        .or(unit.hunt_target.map(|_| ResourceKind::Food))
}
