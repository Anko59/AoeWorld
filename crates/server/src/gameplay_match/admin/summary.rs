use crate::SlotInfo;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, PlayerId, WorldPosition};
use aoe_simulation::{BuildingKind, GameWorld, UnitKind, UnitTask};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

fn occupants(
    building: &aoe_simulation::GameBuilding,
) -> impl Iterator<Item = (aoe_simulation::GameUnit, WorldPosition)> + '_ {
    let at = WorldPosition::new(
        (building.footprint.min.x + building.footprint.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
        (building.footprint.min.y + building.footprint.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
    );
    building.garrison.iter().map(move |g| (g.unit, at))
}

#[derive(Default, Serialize)]
struct Player {
    id: u16,
    kind: &'static str,
    status: &'static str,
    age: String,
    population: usize,
    pop_cap: u16,
    units: BTreeMap<String, usize>,
    buildings: BTreeMap<String, usize>,
    resources: Value,
    town_centers: Vec<Value>,
    army: Value,
    activity: &'static str,
    kills: u64,
    losses: u64,
    #[serde(skip)]
    activities: [usize; 5],
    #[serde(skip)]
    army_sum: (i64, i64, usize),
    #[serde(skip)]
    unit_counts: [usize; aoe_simulation::UNIT_COMBAT.len()],
    #[serde(skip)]
    building_counts: [usize; aoe_simulation::BUILDING_RULES.len()],
}

fn position(at: WorldPosition) -> Value {
    json!({"x": f64::from(at.x) / f64::from(FIXED_SUBUNITS_PER_TILE),
           "y": f64::from(at.y) / f64::from(FIXED_SUBUNITS_PER_TILE)})
}
fn center(rect: aoe_core::TileRect) -> Value {
    json!({"x": (f64::from(rect.min.x) + f64::from(rect.max.x)) / 2.0,
           "y": (f64::from(rect.min.y) + f64::from(rect.max.y)) / 2.0})
}

pub(super) fn match_state(world: &GameWorld, seats: &[SlotInfo]) -> Value {
    let mut players = BTreeMap::<PlayerId, Player>::new();
    for seat in seats {
        players.insert(
            seat.player,
            Player {
                id: seat.player.0,
                kind: if seat.human { "human" } else { "ai" },
                ..Player::default()
            },
        );
    }
    for economy in world.players() {
        let player = players.entry(economy.player).or_insert_with(|| Player {
            id: economy.player.0,
            kind: "ai",
            ..Player::default()
        });
        player.age = format!("{:?}", economy.age);
        player.pop_cap = economy.population_cap;
        let resources = economy.stockpile;
        player.resources = json!({"food": resources.food, "wood": resources.wood,
                                  "gold": resources.gold, "stone": resources.stone});
    }
    // Shelter occupants retain population and identity outside map storage.
    let sheltered = world.buildings().flat_map(occupants);
    for (unit, at) in world
        .units()
        .map(|u| (u, u.position))
        .chain(sheltered)
        .filter(|(u, _)| u.hp > 0)
    {
        let player = players.entry(unit.player).or_insert_with(|| Player {
            id: unit.player.0,
            kind: "ai",
            ..Player::default()
        });
        player.population += 1;
        player.unit_counts[unit.kind as usize] += 1;
        let activity = match unit.task {
            UnitTask::Attacking => 3,
            UnitTask::Building => 1,
            UnitTask::Gathering
            | UnitTask::Returning
            | UnitTask::Farming
            | UnitTask::Hunting
            | UnitTask::Fishing
            | UnitTask::Shepherding => 0,
            _ if unit.moving || unit.planning => 4,
            _ => 2,
        };
        player.activities[activity] += 1;
        if !matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip) {
            player.army_sum.0 += i64::from(at.x);
            player.army_sum.1 += i64::from(at.y);
            player.army_sum.2 += 1;
        }
    }
    for building in world.buildings().filter(|b| b.hp > 0) {
        let player = players.entry(building.player).or_insert_with(|| Player {
            id: building.player.0,
            kind: "ai",
            ..Player::default()
        });
        player.building_counts[building.kind as usize] += 1;
        if building.kind == BuildingKind::TownCenter {
            player.town_centers.push(center(building.footprint));
        }
    }
    for (id, player) in &mut players {
        // Format each occupied kind once per player, rather than allocating
        // a temporary String for every entity while holding the world lock.
        for rule in aoe_simulation::UNIT_COMBAT {
            let count = player.unit_counts[rule.kind as usize];
            if count > 0 {
                player.units.insert(format!("{:?}", rule.kind), count);
            }
        }
        for rule in aoe_simulation::BUILDING_RULES {
            let count = player.building_counts[rule.kind as usize];
            if count > 0 {
                player.buildings.insert(format!("{:?}", rule.kind), count);
            }
        }
        player.status = if player.population == 0 && player.buildings.is_empty() {
            "eliminated"
        } else {
            "alive"
        };
        if player.age.is_empty() {
            player.age = "Dark".into();
        }
        if player.resources.is_null() {
            player.resources = json!({"food":0,"wood":0,"gold":0,"stone":0});
        }
        // Ties follow this documented order; a player without units is idle.
        let names = ["gathering", "building", "idle", "attacking", "marching"];
        let dominant = player
            .activities
            .iter()
            .enumerate()
            .max_by_key(|(i, count)| (**count, std::cmp::Reverse(*i)));
        player.activity = dominant
            .filter(|(_, count)| **count > 0)
            .map_or("idle", |(i, _)| names[i]);
        let (x, y, size) = player.army_sum;
        let centroid = (size > 0).then(|| {
            json!({
                "x": x as f64 / size as f64 / f64::from(FIXED_SUBUNITS_PER_TILE),
                "y": y as f64 / size as f64 / f64::from(FIXED_SUBUNITS_PER_TILE)
            })
        });
        player.army = json!({"size":size,"centroid":centroid});
        (player.kills, player.losses) = world.player_combat_totals(*id);
    }
    let counts = world.combat_telemetry();
    json!({"tick":world.tick().0,"fog_disabled":world.fog_disabled(),
           "players":players.into_values().collect::<Vec<_>>(),
           "combat_per_minute":{"melee_hits":counts.melee_hits,"ranged_hits":counts.ranged_hits,"deaths":counts.deaths}})
}

pub(super) fn player_state(
    world: &GameWorld,
    player: PlayerId,
    offset: usize,
    limit: usize,
) -> Value {
    // Stream across authoritative storage, only allocating the requested page.
    // Ordering is units in storage order then buildings in id order; tick is
    // included so callers can detect changes between pages of a live match.
    let units = world.units().filter(|u| u.player == player && u.hp > 0);
    let buildings = world.buildings().filter(|b| b.player == player && b.hp > 0);
    let mut total = 0;
    let mut entities = Vec::with_capacity(limit);
    for unit in units {
        if total >= offset && entities.len() < limit {
            entities.push(
                json!({"id":unit.id.0,"category":"unit","kind":format!("{:?}",unit.kind),
                "position":position(unit.position),"task":format!("{:?}",unit.task)}),
            );
        }
        total += 1;
    }
    for building in buildings {
        if total >= offset && entities.len() < limit {
            entities.push(json!({"id":building.id.0,"category":"building","kind":format!("{:?}",building.kind),
                "position":center(building.footprint),"task":format!("{:?}",building.state)}));
        }
        total += 1;
    }
    let next = offset.saturating_add(entities.len());
    json!({"id":player.0,"tick":world.tick().0,"offset":offset,"limit":limit,"total":total,
           "next_offset":(next < total).then_some(next),"entities":entities})
}
