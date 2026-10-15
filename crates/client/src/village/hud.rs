use super::{Client, building_center, owner, resource_kind};
use crate::playground::game::{map, position_at};
use aoe_protocol::{GameplayRole, ResourceKind, UnitKind, UnitTask};
use serde::Serialize;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

#[wasm_bindgen(
    inline_js = "export function village_update(payload) { window.aoeInterface?.update(JSON.parse(payload)); }"
)]
extern "C" {
    fn village_update(payload: &str);
}

#[derive(Serialize)]
struct Actor {
    id: u32,
    kind: &'static str,
    x: f64,
    y: f64,
    selected: bool,
    load: Option<(&'static str, u16)>,
}
#[derive(Serialize)]
struct Target {
    kind: u8,
    x: f64,
    y: f64,
    distance: f64,
}
#[derive(Serialize)]
struct Selected {
    id: u32,
    kind: &'static str,
    name: &'static str,
    task: &'static str,
    resource: &'static str,
    amount: u16,
}
#[derive(Serialize)]
struct Hud<'a> {
    stockpile: [u32; 4],
    population: [u16; 2],
    workers: [u16; 4],
    selected: Vec<Selected>,
    queue: Vec<[u64; 2]>,
    seconds: u64,
    observer: bool,
    status: &'a str,
    actors: Vec<Actor>,
    resources: Vec<Target>,
    art: bool,
}

fn name(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Food => "food",
        ResourceKind::Wood => "wood",
        ResourceKind::Gold => "gold",
        ResourceKind::Stone => "stone",
    }
}
fn unit_name(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Scout => "Scout",
        UnitKind::Villager => "Villager",
    }
}
fn task(task: UnitTask) -> &'static str {
    match task {
        UnitTask::Idle => "Idle",
        UnitTask::Moving => "Moving",
        UnitTask::Gathering => "Gathering",
        UnitTask::Carrying => "Carrying",
        UnitTask::Returning => "Returning to Town Center",
    }
}
fn screen(client: &Client, position: [f64; 2]) -> [f64; 2] {
    let p = map::screen_position(client, position);
    [
        p.x * f64::from(client.canvas.client_width()) / f64::from(client.canvas.width()),
        p.y * f64::from(client.canvas.client_height()) / f64::from(client.canvas.height()),
    ]
}

pub(in super::super) fn update(client: &Client) {
    let owner = owner(client);
    let mut hud = Hud {
        stockpile: [0; 4],
        population: [0; 2],
        workers: [0; 4],
        selected: Vec::new(),
        queue: Vec::new(),
        seconds: client.village.tick / u64::from(client.config.tick_hz.max(1)),
        observer: client.role != Some(GameplayRole::Controller),
        status: map::feedback::connection_label(client),
        actors: Vec::new(),
        resources: Vec::new(),
        art: !client.art.village[0].is_empty(),
    };
    if let Some(player) = client.village.players.iter().find(|p| p.player == owner) {
        let s = player.stockpile;
        hud.stockpile = [s.wood, s.food, s.gold, s.stone];
        hud.population = [player.population_used, player.population_cap];
    }
    for unit in client.units.values() {
        let [x, y] = screen(client, position_at(client, unit.id));
        let selected = client.selected.units.contains(&unit.id);
        let kind = resource_kind(client, unit.id);
        if unit.player == owner
            && unit.kind == UnitKind::Villager
            && matches!(
                unit.task,
                UnitTask::Gathering | UnitTask::Carrying | UnitTask::Returning
            )
            && let Some(kind) = kind
        {
            hud.workers[match kind {
                ResourceKind::Wood => 0,
                ResourceKind::Food => 1,
                ResourceKind::Gold => 2,
                ResourceKind::Stone => 3,
            }] += 1;
        }
        hud.actors.push(Actor {
            id: unit.id.0,
            kind: unit_name(unit.kind),
            x,
            y,
            selected,
            load: unit.carried.map(|c| (name(c.kind), c.amount)),
        });
        if selected {
            hud.selected.push(Selected {
                id: unit.id.0,
                kind: unit_name(unit.kind),
                name: unit_name(unit.kind),
                task: task(unit.task),
                resource: kind.map_or("", name),
                amount: unit.carried.map_or(0, |c| c.amount),
            });
        }
    }
    for b in &client.village.buildings {
        let [x, y] = screen(client, building_center(b));
        let selected = client.selected.building_id == Some(b.id);
        hud.actors.push(Actor {
            id: b.id.0,
            kind: "TownCenter",
            x,
            y,
            selected,
            load: None,
        });
        if selected {
            hud.selected.push(Selected {
                id: b.id.0,
                kind: "TownCenter",
                name: "Town Center",
                task: if b.training_queue.is_empty() {
                    "Ready to train"
                } else {
                    "Training Villager"
                },
                resource: "",
                amount: 0,
            });
            hud.queue = b
                .training_queue
                .iter()
                .map(|e| [e.progress_ticks, e.total_ticks])
                .collect();
        }
    }
    let focus = client
        .selected
        .units
        .first()
        .or(client.primary.as_ref())
        .map_or(client.camera.center, |id| position_at(client, *id));
    // Capture targets are bounded to the nearest visible nodes; no world-sized
    // terrain or sprite list is copied for the interface.
    for chunk in client.terrain_chunks.values() {
        for (node, _) in chunk
            .resources()
            .filter(|(n, _)| client.resources.visible(n.id))
        {
            let position = [f64::from(node.tile.x) + 0.5, f64::from(node.tile.y) + 0.5];
            let [x, y] = screen(client, position);
            if x < 0.0
                || y < 54.0
                || x > f64::from(client.canvas.client_width())
                || y > f64::from(client.canvas.client_height()) - 208.0
            {
                continue;
            }
            let distance = (position[0] - focus[0]).powi(2) + (position[1] - focus[1]).powi(2);
            if hud
                .resources
                .iter()
                .filter(|target| target.kind == node.kind as u8)
                .count()
                < 8
            {
                hud.resources.push(Target {
                    kind: node.kind as u8,
                    x,
                    y,
                    distance,
                });
            } else if let Some((slot, furthest)) = hud
                .resources
                .iter()
                .enumerate()
                .filter(|(_, target)| target.kind == node.kind as u8)
                .max_by(|a, b| a.1.distance.total_cmp(&b.1.distance))
                && distance < furthest.distance
            {
                hud.resources[slot] = Target {
                    kind: node.kind as u8,
                    x,
                    y,
                    distance,
                };
            }
        }
    }
    if let Ok(json) = serde_json::to_string(&hud) {
        village_update(&json);
    }
}

pub(in super::super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    for (id, action) in [
        ("train-villager", 0),
        ("stop-units", 1),
        ("move-units", 2),
        ("cancel-training", 3),
    ] {
        if let Some(button) = shared.borrow().document.get_element_by_id(id) {
            let target = shared.clone();
            let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                let mut client = target.borrow_mut();
                match action {
                    0 => super::orders::train(&mut client, false),
                    1 => super::orders::stop(&mut client),
                    2 => {
                        client.village.move_cursor = true;
                        let _ = client.canvas.set_attribute("data-order-mode", "move");
                    }
                    _ => super::orders::train(&mut client, true),
                }
            });
            button.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
            callback.forget();
        }
    }
    Ok(())
}
