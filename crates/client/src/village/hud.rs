use super::{Client, building_center, owner};
use crate::playground::game::{map, position_of};
use aoe_protocol::{BuildingKind, ResourceKind, UnitKind, UnitTask};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function village_update(values, status, details) { window.aoeInterface?.numeric(values, status); window.aoeInterface?.details(details); }"
)]
extern "C" {
    fn village_update(values: &[f64], status: &str, details: &[f64]);
}

const BUILDING_OFFSET: usize = UnitKind::ALL.len();
const RELIC_KIND: usize = ANIMAL_OFFSET + aoe_protocol::AnimalKind::Wolf as usize + 1;
const ANIMAL_OFFSET: usize = BUILDING_OFFSET + BuildingKind::ALL.len();

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

pub(in super::super) fn update(client: &Client) {
    let owner = owner(client);
    let mut fighters = super::selection::owned_units(client)
        .filter(|unit| super::orders::can_choose_stance(unit.kind));
    let stance = fighters
        .next()
        .filter(|first| fighters.all(|unit| unit.stance == first.stance))
        .map_or("", |u| ["0", "1", "2", "3"][u.stance as usize]);
    if client
        .canvas
        .get_attribute("data-selected-stance")
        .as_deref()
        != Some(stance)
    {
        let _ = client.canvas.set_attribute("data-selected-stance", stance);
    }
    let overview = crate::playground::game::strategic::active(client);
    // Header: four stockpiles, used/cap, four worker counts, clock,
    // native-art flag, actor count and the two idle counts. Each actor
    // record contains nineteen numbers. Production rows follow the actors.
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
    // Keep one terrain snapshot for the entire HUD update. Empty and
    // strategic views can avoid preparing the detailed scene altogether.
    let scene = std::cell::OnceCell::new();
    let screen = |position, selected| {
        crate::hud_visibility::project_if_horizontal(
            client.camera,
            position,
            scale,
            viewport,
            selected,
            || {
                let height = if overview {
                    client.camera.focus_elevation_meters
                } else {
                    scene
                        .get_or_init(|| map::scene::prepare(client))
                        .height(position)
                        .unwrap_or_else(|| map::elevation_at_world(client, position))
                };
                let point = client.camera.world_to_screen_at_height(position, height);
                [point.x * scale[0], point.y * scale[1]]
            },
        )
    };
    let camera = aoe_rendering::SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let heads = if overview {
        std::collections::HashMap::new()
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
            .collect()
    };
    let head = |id, y| {
        heads
            .get(&id)
            .copied()
            .unwrap_or(y - 42.0 * client.camera.zoom)
    };
    let mut values = vec![0.0; 15];
    values[13] = client.village.idle_villagers.len() as f64;
    values[14] = client.village.idle_military.len() as f64;
    values[10] = (client.village.tick / u64::from(client.config.tick_hz.max(1))) as f64;
    values[11] = flag(client.art.compositions.iter().any(|b| !b.parts.is_empty()));
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
    let selection: std::collections::HashSet<_> = client.selected.units.iter().copied().collect();
    for unit in client.units.values() {
        let selected = selection.contains(&unit.id);
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
        let Some([x, y]) = screen(position, selected) else {
            continue;
        };
        let top = head(unit.id, y);
        if !crate::hud_visibility::visible([x, y], top, viewport, selected) {
            continue;
        }
        values[12] += 1.0;
        let resource = unit.carried.map(|load| load.kind).or(unit.gathering);
        // Only the selection panel reads combat stats. The marker stream
        // still carries each visible actor's health, task, load and owner.
        let [attack, melee, pierce, capacity, range] = if selected {
            let rules = crate::building_ui::content::rules();
            let base = rules.units[unit.kind as usize].combat;
            let player = client
                .village
                .players
                .iter()
                .find(|p| p.player == unit.player);
            let researched = player.map_or(0, |p| p.researched);
            let stats = aoe_simulation::apply_research_stats(
                base,
                player.is_some_and(|p| p.age >= aoe_protocol::Age::Feudal),
                researched,
            );
            let loom = researched & 1 != 0 && unit.kind == UnitKind::Villager;
            let capacity = if unit.kind == UnitKind::FishingShip {
                15
            } else {
                let capacity = rules.villager_capacity(researched)
                    + u16::from(
                        unit.task == UnitTask::Farming
                            && researched & aoe_protocol::Tech::HeavyPlow.bit() != 0,
                    );
                capacity.max(unit.carried.map_or(0, |load| load.amount))
            };
            [
                f64::from(stats.attack),
                f64::from(stats.melee_armour) + flag(loom),
                f64::from(stats.pierce_armour) + 2.0 * flag(loom),
                f64::from(capacity),
                f64::from(stats.range),
            ]
        } else {
            [0.0; 5]
        };
        values.extend_from_slice(&[
            f64::from(unit.id.0),
            unit.kind as u8 as f64,
            x,
            y,
            flag(selected),
            match unit.native_action {
                Some(aoe_protocol::NativeAction::TrebuchetTransform(_))
                    if unit.kind == UnitKind::Trebuchet =>
                {
                    18.0
                }
                Some(aoe_protocol::NativeAction::TrebuchetTransform(_)) => 19.0,
                _ => unit.task as u8 as f64,
            },
            if unit.kind == UnitKind::Monk && super::relics::carries(client, unit.id) {
                5.0
            } else {
                resource.map_or(4.0, |kind| kind as u8 as f64)
            },
            f64::from(unit.carried.map_or(0, |load| load.amount)),
            f64::from(unit.hp),
            f64::from(unit.max_hp),
            attack,
            melee,
            pierce,
            top,
            capacity,
            range,
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
        let Some([x, y]) = screen(building_center(building), selected) else {
            continue;
        };
        if !crate::hud_visibility::visible([x, y], y - 48.0, viewport, selected) {
            continue;
        }
        values[12] += 1.0;
        values.extend_from_slice(&[
            f64::from(building.id.0),
            (BUILDING_OFFSET + building.kind as usize) as f64,
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
        let Some([x, y]) = screen(animal.position.as_tiles(), selected) else {
            continue;
        };
        let top = head(animal.id, y);
        if !crate::hud_visibility::visible([x, y], top, viewport, selected) {
            continue;
        }
        values[12] += 1.0;
        let (hp, attack) = match animal.kind {
            aoe_protocol::AnimalKind::Sheep => (7, 0),
            aoe_protocol::AnimalKind::Deer => (5, 0),
            aoe_protocol::AnimalKind::Boar => (75, 7),
            aoe_protocol::AnimalKind::Wolf => (25, 3),
        };
        values.extend_from_slice(&[
            f64::from(animal.id.0),
            (ANIMAL_OFFSET + animal.kind as usize) as f64,
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
    for relic in super::relics::ground(client) {
        let selected = client.selected.units.contains(&relic.id);
        if overview && !selected {
            continue;
        }
        let Some([x, y]) = screen(relic.position.as_tiles(), selected) else {
            continue;
        };
        if !crate::hud_visibility::visible([x, y], y, viewport, selected) {
            continue;
        }
        let mut actor = [0.0; 19];
        actor[..6].copy_from_slice(&[
            f64::from(relic.id.0),
            RELIC_KIND as f64,
            x,
            y,
            flag(selected),
            13.0,
        ]);
        actor[13] = y;
        actor[16] = -1.0;
        values[12] += 1.0;
        values.extend(actor);
    }
    let group = client.selected.buildings.len() > 1;
    let mut producers: Vec<_> = client
        .village
        .buildings
        .iter()
        .filter(|building| {
            building.player == owner
                && if group {
                    building.hp > 0
                        && client.selected.has_building(building.id)
                        && UnitKind::ALL.iter().any(|kind| {
                            crate::building_ui::rules(building.kind).trains(*kind as u8)
                        })
                } else {
                    Some(building.id) == client.selected.building_id
                }
        })
        .collect();
    producers.sort_unstable_by_key(|building| building.id);
    for building in producers {
        let queue = &building.training_queue;
        let (elapsed, total) = queue.first().map_or((0.0, 0.0), |entry| {
            (entry.progress_ticks as f64, entry.total_ticks as f64)
        });
        values.extend([
            f64::from(building.id.0),
            building.kind as u8 as f64,
            queue.len() as f64,
            elapsed,
            total,
        ]);
        values.extend(queue.iter().map(|entry| entry.kind as u8 as f64));
    }
    let details = super::presentation::values(client);
    village_update(&values, map::feedback::connection_label(client), &details);
    super::relics::update(client);
    super::building_panel::update(client, panel_scale);
    let activity = crate::kingdom_activity::summarize(&client.village.buildings, owner);
    let mut rows = Vec::with_capacity(activity.len() * 5);
    for entry in activity {
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
