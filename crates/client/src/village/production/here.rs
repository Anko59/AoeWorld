//! A bounded deterministic plan, executed through ordinary rally/train orders.
use super::*;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, ScreenPoint, WorldPosition};
use aoe_protocol::{GameplayClientMessage as Message, RallyTarget, Stockpile};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Copy)]
pub(super) struct HereRequest {
    pub kind: UnitKind,
    pub count: usize,
    pub position: WorldPosition,
}

fn spend(stockpile: &mut Stockpile, population: &mut u32, cost: Stockpile) {
    stockpile.food = stockpile.food.saturating_sub(cost.food);
    stockpile.wood = stockpile.wood.saturating_sub(cost.wood);
    stockpile.gold = stockpile.gold.saturating_sub(cost.gold);
    stockpile.stone = stockpile.stone.saturating_sub(cost.stone);
    *population = population.saturating_add(1);
}

fn distance(b: &GameplayBuildingState, p: WorldPosition) -> u64 {
    let dx = ((i64::from(b.footprint.min.x) + i64::from(b.footprint.max.x))
        * (i64::from(FIXED_SUBUNITS_PER_TILE) / 2)
        - i64::from(p.x))
    .unsigned_abs();
    let dy = ((i64::from(b.footprint.min.y) + i64::from(b.footprint.max.y))
        * (i64::from(FIXED_SUBUNITS_PER_TILE) / 2)
        - i64::from(p.y))
    .unsigned_abs();
    dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
}

pub(super) fn plan(
    buildings: &[GameplayBuildingState],
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    request: HereRequest,
    pending: &PendingTraining,
    occupied: impl Fn(&GameplayBuildingState) -> bool,
) -> Vec<EntityId> {
    let Some(player) = player else {
        return Vec::new();
    };
    let Some(cost) = availability::training_cost(request.kind) else {
        return Vec::new();
    };
    let mut stockpile = player.stockpile;
    let mut population = player.population_used;
    for entry in &pending.entries {
        if entry.2.is_none_or(|tick| tick > pending.observed) {
            spend(
                &mut stockpile,
                &mut population,
                availability::training_cost(entry.3).unwrap_or_default(),
            );
        }
    }
    let queued: usize = buildings
        .iter()
        .filter(|b| b.player == owner && b.hp > 0)
        .map(|b| b.training_queue.len())
        .sum();
    population = population.saturating_add(queued as u32);
    let mut rally_count = 0;
    let mut result = Vec::new();
    for _ in 0..request.count.min(50) {
        if population >= player.population_cap
            || !matches!(availability::cost(stockpile, cost), Availability::Ready)
        {
            break;
        }
        let queued = |b: &GameplayBuildingState| {
            b.training_queue.len()
                + pending.count(b.id)
                + result.iter().filter(|id| **id == b.id).count()
        };
        let next = buildings
            .iter()
            .filter(|b| {
                b.hp > 0
                    && queued(b) < aoe_protocol::MAX_TRAINING_QUEUE
                    && (request.kind != UnitKind::Vassal
                        || (!occupied(b) && pending.count(b.id) == 0 && !result.contains(&b.id)))
                    && matches!(
                        availability::training(b, Some(player), owner, request.kind),
                        Availability::Ready
                    )
            })
            .min_by_key(|b| (queued(b), distance(b, request.position), b.id));
        let Some(building) = next else {
            break;
        };
        let new_rally = !result.contains(&building.id);
        let messages =
            pending.entries.len() + result.len() + rally_count + 1 + usize::from(new_rally);
        if messages > aoe_protocol::MAX_PENDING_COMMANDS_PER_CONNECTION {
            break;
        }
        rally_count += usize::from(new_rally);
        result.push(building.id);
        spend(&mut stockpile, &mut population, cost);
    }
    result
}

#[wasm_bindgen(inline_js = "
export function here_take(values) { const v = window.aoeNavigation?.production?.take(); if (v) values.set(v); }
export function here_result(count) { window.aoeNavigation?.production?.result(count); }
")]
extern "C" {
    fn here_take(values: &mut [f64]);
    fn here_result(count: usize);
}

pub(in super::super::super) fn click(client: &mut Client, point: ScreenPoint) -> bool {
    if !crate::playground::game::strategic::active(client) {
        return false;
    }
    let mut values = [f64::NAN; 2];
    here_take(&mut values);
    if !values.iter().all(|v| v.is_finite()) {
        return false;
    }
    let Some(kind) = UnitKind::ALL.get(values[0] as usize).copied() else {
        return true;
    };
    let world = crate::playground::game::strategic::world_at_screen(client, point);
    if !world.iter().all(|value| value.is_finite())
        || world[0] < 0.0
        || world[1] < 0.0
        || world[0] >= f64::from(client.config.width_tiles)
        || world[1] >= f64::from(client.config.height_tiles)
    {
        here_result(0);
        return true;
    }
    let Ok(position) = WorldPosition::from_i64(
        (world[0] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
        (world[1] * f64::from(FIXED_SUBUNITS_PER_TILE)).round() as i64,
    ) else {
        return true;
    };
    let position = client.config.snap_ground_position(position);
    let request = HereRequest {
        kind,
        count: (values[1] as usize).clamp(1, 50),
        position,
    };
    let player = owner(client);
    let orders = plan(
        &client.village.buildings,
        client.village.players.iter().find(|p| p.player == player),
        player,
        request,
        &client.village.pending_training,
        |b| {
            super::has_vassal(b)
                || client.units.values().any(|u| {
                    u.hp > 0
                        && u.player == player
                        && u.kind == UnitKind::Vassal
                        && u.home_castle == Some(b.id)
                })
        },
    );
    let count = orders.len();
    for (at, &id) in orders.iter().enumerate() {
        if !orders[..at].contains(&id) {
            super::super::orders::send(
                client,
                Message::SetRallyOrder {
                    sequence: client.next_sequence,
                    building_id: id,
                    target: RallyTarget::Ground(position),
                },
            );
        }
        let sequence = client.next_sequence;
        super::super::orders::send(
            client,
            Message::TrainOrder {
                sequence,
                building_id: id,
                kind,
            },
        );
        client.village.pending_training.record(sequence, id, kind);
    }
    here_result(count);
    true
}
