use super::{Client, building_center, owner, resource_kind};
use crate::playground::game::{map, position_at};
use aoe_protocol::{GameplayRole, ResourceKind, UnitKind, UnitTask};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};

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
    let owner = owner(client);
    // Header: four stockpiles, used/cap, four worker counts, clock, observing,
    // native-art flag, then actor/queue/resource counts. Actor records are eight
    // numbers. Decode immediately in JS; never retain a view of WASM memory.
    let mut values = vec![0.0; 16];
    values[10] = (client.village.tick / u64::from(client.config.tick_hz.max(1))) as f64;
    values[11] = flag(client.role != Some(GameplayRole::Controller));
    values[12] = flag(!client.art.village[0].is_empty());
    values[13] = (client.units.len() + client.village.buildings.len()) as f64;
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
        let position = position_at(client, unit.id);
        let [x, y] = screen(client, position);
        let resource = unit.carried.map(|load| load.kind).or(unit.gathering);
        if unit.player == owner
            && unit.kind == UnitKind::Villager
            && counts_as_gatherer(unit.task)
            && let Some(kind) = resource_kind(client, unit.id)
        {
            values[6 + match kind {
                ResourceKind::Wood => 0,
                ResourceKind::Food => 1,
                ResourceKind::Gold => 2,
                ResourceKind::Stone => 3,
            }] += 1.0;
        }
        values.extend_from_slice(&[
            f64::from(unit.id.0),
            match unit.kind {
                UnitKind::Scout => 0.0,
                UnitKind::Villager => 1.0,
                UnitKind::Militia => 11.0,
            },
            x,
            y,
            flag(selected),
            unit.task as u8 as f64,
            resource.map_or(4.0, |kind| kind as u8 as f64),
            f64::from(unit.carried.map_or(0, |load| load.amount)),
        ]);
    }
    for building in &client.village.buildings {
        let [x, y] = screen(client, building_center(building));
        values.extend_from_slice(&[
            f64::from(building.id.0),
            2.0 + building.kind as u8 as f64,
            x,
            y,
            flag(client.selected.building_id == Some(building.id)),
            if building.training_queue.is_empty() {
                5.0
            } else {
                6.0
            },
            4.0,
            0.0,
        ]);
    }
    if let Some(building) = client
        .village
        .buildings
        .iter()
        .find(|b| Some(b.id) == client.selected.building_id)
    {
        values[14] = building.training_queue.len() as f64;
        for entry in &building.training_queue {
            values.extend_from_slice(&[entry.progress_ticks as f64, entry.total_ticks as f64]);
        }
    }
    let details = super::presentation::values(client);
    village_update(&values, map::feedback::connection_label(client), &details);
    super::building_panel::update(client);
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
    super::building_panel::install(shared)?;
    Ok(())
}

/// Villagers working a resource, including farmers standing on a farm.
pub(super) fn counts_as_gatherer(task: UnitTask) -> bool {
    matches!(
        task,
        UnitTask::Gathering | UnitTask::Returning | UnitTask::Farming
    )
}
