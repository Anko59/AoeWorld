//! Where food comes from: sheep first, then berries (with a Mill beside them),
//! then deer and boar, then farms once the land runs dry.
use std::collections::{BTreeMap, BTreeSet};

use aoe_sdk::{AnimalKind, BuildingKind, EntityId, Order, ResourceKind, UnitTask};

use super::{
    ASSIGN_PAUSE_SECONDS, Done, FOOD, HUNT_STALL_SECONDS, HUNT_TRIES, best_node, gather,
    nearest_center, shares, since,
};
use crate::context::Context;
use crate::geometry::{dist, nearest};
use crate::play::{buildorder, constructor};
use crate::roster::building_price;
use crate::view::{Animal, Building, Tile, Unit};

/// Sheep first, then berries (with a Mill beside them), then deer and boar,
/// then farms once the land runs dry.
pub fn send_to_food(ctx: &mut Context<'_>, villager: &Unit) -> Done<bool> {
    if let Some(sheep) = huntable(ctx, &[AnimalKind::Sheep], Some(villager.tile())) {
        return hunt(ctx, villager, &sheep);
    }
    if let Some(node) = best_node(ctx, villager, ResourceKind::Food) {
        if ctx.dropoff_distance(&node) > f64::from(ctx.config.dropoff_distance)
            && constructor::ensure_camp(ctx, BuildingKind::Mill, &node, villager.id)?
        {
            return Ok(true);
        }
        gather(ctx, villager, &node)?;
        return Ok(true);
    }
    let game = huntable(
        ctx,
        &[AnimalKind::Deer, AnimalKind::Boar],
        Some(villager.tile()),
    );
    if let Some(game) = game {
        return hunt(ctx, villager, &game);
    }
    send_to_farm(ctx, villager)
}

/// Hunt order that gives up on an animal one villager cannot finish.
///
/// The count is per villager: a quarried animal is picked by every idle worker
/// within the same second, so counting orders alone would write off a herd
/// beside the Town Center after a dozen villagers had each been sent at it
/// once.
fn hunt(ctx: &mut Context<'_>, villager: &Unit, animal: &Animal) -> Done<bool> {
    let tries = ctx
        .board
        .hunt_tries
        .entry((villager.id, animal.id))
        .or_insert(0);
    *tries += 1;
    if *tries > HUNT_TRIES {
        ctx.board.bad_resources.insert(u64::from(animal.id));
        ctx.board.bump("hunts_abandoned");
    }
    let order = Order::Hunt {
        units: vec![EntityId(villager.id)],
        animal: EntityId(animal.id),
    };
    ctx.order_or_stop("hunt", order)?;
    ctx.board.hunters.insert(villager.id, animal.id);
    Ok(true)
}

/// Hunters never fall idle, so a quarry they cannot reach or catch would hold
/// them for ever: when neither its health nor its meat has changed for a
/// while, write it off and stand the hunters down for a new job.
pub fn drop_stuck_hunts(ctx: &mut Context<'_>) -> Done {
    let hunting: BTreeSet<u32> = ctx
        .villagers
        .iter()
        .filter(|v| v.task == UnitTask::Hunting)
        .map(|v| v.id)
        .collect();
    ctx.board.hunters.retain(|id, _| hunting.contains(id));
    let animals: BTreeMap<u32, Animal> =
        ctx.world.animals().into_iter().map(|a| (a.id, a)).collect();
    let limit = (HUNT_STALL_SECONDS * ctx.hz()) as i64;
    let quarries: BTreeSet<u32> = ctx.board.hunters.values().copied().collect();
    for animal_id in quarries {
        let Some(animal) = animals.get(&animal_id) else {
            continue;
        };
        let state = (animal.hp, animal.food_remaining);
        let tick = ctx.tick;
        match ctx.board.hunt_watch.get(&animal_id) {
            Some((seen, since_tick)) if *seen == state => {
                if since(tick, *since_tick) <= limit {
                    continue;
                }
                ctx.board.bad_resources.insert(u64::from(animal_id));
                ctx.board.bump("hunts_abandoned");
                let stopped: Vec<u32> = ctx
                    .board
                    .hunters
                    .iter()
                    .filter(|(_, a)| **a == animal_id)
                    .map(|(v, _)| *v)
                    .collect();
                for villager in stopped {
                    let order = Order::Stop {
                        unit: EntityId(villager),
                    };
                    ctx.order_or_stop("stop", order)?;
                    ctx.board.hunters.remove(&villager);
                    ctx.board.assigned.remove(&villager);
                }
            }
            _ => {
                ctx.board.hunt_watch.insert(animal_id, (state, tick));
            }
        }
    }
    Ok(())
}

/// The nearest animal of the first kind that has one to hunt.
fn huntable(ctx: &Context<'_>, kinds: &[AnimalKind], origin: Option<Tile>) -> Option<Animal> {
    let origin = origin.or(ctx.home)?;
    let me = ctx.world.me();
    let taken: BTreeSet<u32> = ctx.board.hunters.values().copied().collect();
    for kind in kinds {
        let animals: Vec<Animal> = ctx
            .world
            .animals()
            .into_iter()
            .filter(|a| {
                a.kind == *kind
                    && (a.owner.is_none() || a.owner == me)
                    && a.food_remaining > 0
                    && dist(a.tile(), origin) <= f64::from(ctx.config.hunt_distance)
                    && !ctx.board.bad_resources.contains(&u64::from(a.id))
                    // One hunter per quarry: the rest of a herd, or the wood
                    // and gold behind it, needs workers too.
                    && !taken.contains(&a.id)
            })
            .collect();
        if let Some(found) = nearest(&animals, origin, |a| a.tile()) {
            return Some(found.clone());
        }
    }
    None
}

pub fn forget_farmers(ctx: &mut Context<'_>) {
    let pause = (ASSIGN_PAUSE_SECONDS * ctx.hz()) as i64;
    let farms: Vec<&Building> = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Farm)
        .collect();
    let tick = ctx.tick;
    let board = &mut *ctx.board;
    let stale: Vec<u32> = board
        .farmers
        .iter()
        .filter(|(farm, worker)| {
            let Some(unit) = ctx.villagers.iter().find(|v| v.id == **worker) else {
                return true;
            };
            let assigned = board.assigned.get(*worker).map(|t| since(tick, *t));
            !farms.iter().any(|f| f.id == **farm)
                || (unit.gathering.is_none()
                    && unit.task != UnitTask::Farming
                    && assigned.is_none_or(|age| age >= pause))
        })
        .map(|(farm, _)| *farm)
        .collect();
    for farm in stale {
        board.farmers.remove(&farm);
    }
    // Construction automatically puts a farm's builder to work. That job did
    // not originate in send_to_farm, but its visible work pose and tile
    // identify the occupied plot, including after a reconnect.
    for worker in &ctx.villagers {
        if worker.task != UnitTask::Farming {
            continue;
        }
        let (x, y) = worker.tile();
        if let Some(farm) = farms
            .iter()
            .find(|f| f.x <= x && x < f.x + f.width && f.y <= y && y < f.y + f.height)
        {
            board.farmers.insert(farm.id, worker.id);
        }
    }
}

fn send_to_farm(ctx: &mut Context<'_>, villager: &Unit) -> Done<bool> {
    let free: Vec<Building> = ctx
        .world
        .my_buildings()
        .into_iter()
        .filter(|f| {
            f.kind == BuildingKind::Farm
                && f.complete
                && !ctx.board.farmers.contains_key(&f.id)
                && !ctx.board.cooling(&format!("farm:{}", f.id), ctx.tick)
        })
        .collect();
    if let Some(farm) = nearest(&free, villager.tile(), |b| b.center()) {
        ctx.board.farmers.insert(farm.id, villager.id);
        let order = Order::Farm {
            unit: EntityId(villager.id),
            farm: EntityId(farm.id),
        };
        ctx.order_or_stop("farm", order)?;
        return Ok(true);
    }
    let mills: Vec<&Building> = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Mill && b.complete)
        .collect();
    let mill = nearest(&mills, villager.tile(), |b| b.center()).map(|b| b.center());
    let cfg = ctx.config;
    if let Some(mill) = mill
        && dist(mill, villager.tile()) > f64::from(cfg.dropoff_distance * 2)
    {
        let near = nearest_center(ctx, villager.tile());
        if constructor::pending(ctx, BuildingKind::Mill) == 0
            && let Some(near) = near
            && constructor::place(ctx, BuildingKind::Mill, near, &[villager.id], Some(10), 1)?
        {
            return Ok(true);
        }
    }
    let Some(mill) = mill else {
        if constructor::total(ctx, BuildingKind::Mill) == 0 {
            let Some(home) = ctx.home else {
                return Ok(false);
            };
            return constructor::place(ctx, BuildingKind::Mill, home, &[villager.id], Some(10), 1);
        }
        return Ok(false);
    };
    if constructor::pending(ctx, BuildingKind::Farm) >= 2 {
        return Ok(false);
    }
    let farms = constructor::total(ctx, BuildingKind::Farm);
    let wanted = (ctx.villagers.len() as f64 * shares(ctx)[FOOD]).round_ties_even();
    if farms as f64 >= f64::from(cfg.farm_target).max(wanted) {
        return Ok(false);
    }
    let cost = building_price(BuildingKind::Farm);
    let reserve = buildorder::farm_wood_reserve(ctx);
    if !ctx.can_afford([cost[0], cost[1] + reserve, cost[2], cost[3]]) {
        return Ok(false);
    }
    constructor::place(ctx, BuildingKind::Farm, mill, &[villager.id], Some(9), 0)
}
