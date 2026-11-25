//! Age advances and research from the public technology tree
//! (`ai/aoe_ai/play/research.py`).
use std::collections::BTreeSet;

use aoe_sdk::rules::Technology;
use aoe_sdk::{Age, BuildingKind, EntityId, Order, Tech, UnitKind};

use super::buildorder::{self, Stage, stat};
use super::management::funding_reserve;
use crate::context::{Context, Lines, OrderBudgetExhausted};
use crate::roster::{age_rank, land_producers, profile, research_price, technology_rows};
use crate::view::Building;

type Done<T = ()> = Result<T, OrderBudgetExhausted>;

const AGE_TECHS: [Tech; 4] = [
    Tech::Loom,
    Tech::FeudalAge,
    Tech::CastleAge,
    Tech::ImperialAge,
];
const ARMS_RESEARCH: [BuildingKind; 2] = [BuildingKind::Blacksmith, BuildingKind::University];

/// The finished buildings free to research, by kind in order of first sight.
type Sites = Vec<(BuildingKind, Vec<Building>)>;

fn available(sites: &mut Sites, kind: BuildingKind) -> Option<&mut Vec<Building>> {
    sites.iter_mut().find(|(k, _)| *k == kind).map(|(_, b)| b)
}

fn has_site(sites: &Sites, kind: BuildingKind) -> bool {
    sites.iter().any(|(k, _)| *k == kind)
}

pub fn step(ctx: &mut Context<'_>) -> Done {
    let (Some(eco), Some(center)) = (ctx.economy.clone(), ctx.center.clone()) else {
        return Ok(());
    };
    let stage = buildorder::stage(ctx);
    if stage == Stage::ImperialAdvance && center.researching.is_none() {
        // Everything for the age stands: hold villagers and soldiers until it
        // is bought, then resume while it researches.
        ctx.saving_castle = true;
        ctx.hold_military_training = true;
        ctx.hold_town_center = true;
        if !research(ctx, Tech::ImperialAge, &center)? {
            ctx.reserve(research_price(Tech::ImperialAge));
        }
        return Ok(());
    }
    if stage == Stage::CastleAdvance {
        ctx.saving_castle = true;
        ctx.hold_town_center = true;
        research(ctx, Tech::CastleAge, &center)?;
        return Ok(());
    }
    if buildorder::wants_castle_food(ctx) {
        // The Feudal army may keep growing, but villagers and optional
        // upgrades must not consume the age's food reserve meanwhile.
        ctx.hold_town_center = true;
        if ctx.config.castle_first {
            ctx.saving_castle = true; // no soldiers until the age is bought
        }
        return Ok(());
    }
    if stage == Stage::Advance {
        // Everything for the age is standing: stop spending on anything but
        // the age and let food pile up to its price.
        if !research(ctx, Tech::FeudalAge, &center)? {
            ctx.hold_town_center = true;
        }
    } else if !eco.has_researched(Tech::Loom) && buildorder::wants_loom(ctx) {
        research(ctx, Tech::Loom, &center)?;
    }
    if ctx.feudal() {
        upgrades(ctx)?;
    }
    Ok(())
}

fn research(ctx: &mut Context<'_>, tech: Tech, building: &Building) -> Done<bool> {
    let key = format!("research:{tech:?}");
    if ctx.buildings.iter().any(|b| b.researching == Some(tech))
        || building.researching.is_some()
        || ctx.board.cooling(&key, ctx.tick)
    {
        return Ok(false);
    }
    let cost = research_price(tech);
    if ctx.can_afford(cost) {
        if building.queue_length == 0 {
            ctx.spend(cost)?;
            let order = Order::Research {
                building: EntityId(building.id),
                tech,
            };
            ctx.order_or_stop("research", order)?;
            ctx.draining.insert(building.id);
            let wait = 3 * ctx.hz();
            ctx.board.cool(&key, ctx.tick, wait);
            return Ok(true);
        }
        // Affordable but the queue is busy: stop feeding it so it drains.
        if building.kind == BuildingKind::TownCenter {
            ctx.hold_town_center = true;
        }
    }
    Ok(false)
}

fn upgrades(ctx: &mut Context<'_>) -> Done {
    let Some(eco) = ctx.economy.clone() else {
        return Ok(());
    };
    let researched = &eco.researched;
    let mut active: Vec<Tech> = ctx.buildings.iter().filter_map(|b| b.researching).collect();
    let mut pending_buildings = BTreeSet::new();
    for tracked in ctx.board.tracked.values() {
        if let Order::Research { building, tech } = &tracked.order {
            active.push(*tech);
            pending_buildings.insert(building.0);
        }
    }
    let mut sites: Sites = Vec::new();
    let mut complete: Vec<BuildingKind> = Vec::new();
    let mut researching_kinds: Vec<BuildingKind> = Vec::new();
    for building in ctx.buildings.iter().filter(|b| b.complete) {
        if !complete.contains(&building.kind) {
            complete.push(building.kind);
        }
        if building.researching.is_none() && !pending_buildings.contains(&building.id) {
            match available(&mut sites, building.kind) {
                Some(list) => list.push(building.clone()),
                None => sites.push((building.kind, vec![building.clone()])),
            }
        } else if !researching_kinds.contains(&building.kind) {
            researching_kinds.push(building.kind);
        }
    }
    let candidates: Vec<&Technology> = technology_rows()
        .iter()
        .filter(|row| {
            !AGE_TECHS.contains(&row.tech)
                && !researched.contains(&row.tech)
                && !active.contains(&row.tech)
                && has_site(&sites, row.building)
                && age_rank(row.age) <= age_rank(eco.age)
                && row.prerequisite.is_none_or(|p| researched.contains(&p))
                && row
                    .building_kinds
                    .iter()
                    .filter(|k| complete.contains(k))
                    .count()
                    >= row.building_count
                && !ctx
                    .board
                    .cooling(&format!("research:{:?}", row.tech), ctx.tick)
        })
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }
    let fielded = ctx.combat_lines().clone();
    let castle_opening = ctx.castle() && !ctx.imperial() && stat(ctx, "castle_units_peak") == 0;
    if castle_opening {
        let started: Vec<Tech> = active.iter().chain(researched).copied().collect();
        if first_castle_upgrade(ctx, &candidates, &started, &sites, &fielded)? {
            return Ok(());
        }
    }

    let reserve = funding_reserve(ctx);
    let food_float = ctx.config.optional_research_food_float;
    let wood_float =
        ctx.config.optional_research_wood_float + buildorder::castle_prerequisite_wood(ctx);
    let mut gold_goal = 0;
    let producers =
        |kind: BuildingKind| kind == BuildingKind::Dock || land_producers().contains(&kind);
    let mut ranked: Vec<_> = candidates
        .iter()
        .map(|row| (priority(row, &fielded), *row))
        .collect();
    ranked.sort_by_key(|(priority, _)| *priority);
    for (priority, row) in ranked {
        let free_count = available(&mut sites, row.building).map_or(0, |list| list.len());
        if free_count == 0 || (producers(row.building) && researching_kinds.contains(&row.building))
        {
            continue;
        }
        if row.building == BuildingKind::TownCenter
            && ctx.castle()
            && ctx.villagers.len() < ctx.config.imperial_villagers as usize
            && free_count == 1
        {
            continue;
        }
        if !ctx.imperial() && land_producers().contains(&row.building) {
            let affects_army = row.upgrades.iter().any(|line| fielded.get(*line) > 0)
                || row
                    .improves
                    .iter()
                    .any(|unit| fielded.get(profile(*unit).line) > 0);
            if castle_opening || !affects_army {
                continue;
            }
        }
        let cost = research_price(row.tech);
        let arms = !row.upgrades.is_empty()
            || !row.unlocks.is_empty()
            || !row.improves.is_empty()
            || ARMS_RESEARCH.contains(&row.building);
        // Fielded lines and new unlocks earn their actual price first in
        // Imperial. The first management units still precede optional tech.
        let needs_management_budget = !(ctx.imperial() && priority.0 <= 1);
        let mut needed = cost;
        if needs_management_budget {
            needed = std::array::from_fn(|i| cost[i] + reserve[i]);
        }
        if !arms || !ctx.castle() {
            needed[0] += food_float;
            needed[1] += wood_float;
        }
        if !ctx.can_afford(needed) {
            if arms
                && ctx.imperial()
                && gold_goal == 0
                && ctx.can_afford([needed[0], needed[1], 0, needed[3]])
            {
                // Hold the gold now, so cheaper rows later in this pass cannot
                // spend what the higher-priority upgrade waits for.
                gold_goal = cost[2];
                let free = eco.gold.saturating_sub(ctx.spent[2]);
                ctx.spent[2] += free.min(gold_goal);
            }
            continue;
        }
        let Some(list) = available(&mut sites, row.building) else {
            continue;
        };
        let Some(building) = least_queued(list).cloned() else {
            continue;
        };
        claim(ctx, row, &building)?;
        if !ctx.imperial() {
            return Ok(());
        }
        if producers(row.building) || !building.idle {
            list.clear(); // keep sibling military producers training
        } else {
            list.retain(|b| b.id != building.id);
        }
    }
    Ok(())
}

/// The building with the shortest queue; the lowest id among equals.
fn least_queued(list: &[Building]) -> Option<&Building> {
    list.iter().min_by_key(|b| (b.queue_length, b.id))
}

/// Fund the opening Castle upgrade before spending the next food delivery.
fn first_castle_upgrade(
    ctx: &mut Context<'_>,
    candidates: &[&Technology],
    started: &[Tech],
    sites: &Sites,
    fielded: &Lines,
) -> Done<bool> {
    if technology_rows().iter().any(|row| {
        row.age == Age::Castle && !row.upgrades.is_empty() && started.contains(&row.tech)
    }) {
        return Ok(false);
    }
    let upgrades: Vec<&Technology> = candidates
        .iter()
        .copied()
        .filter(|row| {
            row.age == Age::Castle
                && !row.upgrades.is_empty()
                && land_producers().contains(&row.building)
        })
        .collect();
    let mut chosen = upgrades
        .iter()
        .find(|row| row.upgrades.iter().any(|line| fielded.get(*line) > 0))
        .copied();
    if chosen.is_none() && ctx.army.len() + ctx.led.len() >= ctx.config.castle_army_min as usize {
        let core: Vec<UnitKind> = ctx.config.army_mix.iter().map(|(kind, _)| *kind).collect();
        chosen = upgrades
            .iter()
            .find(|row| row.upgrades.iter().any(|line| core.contains(line)))
            .copied();
    }
    let Some(row) = chosen else {
        return Ok(false);
    };
    let Some(building) = sites
        .iter()
        .find(|(kind, _)| *kind == row.building)
        .and_then(|(_, list)| least_queued(list))
        .cloned()
    else {
        return Ok(false);
    };
    let cost = research_price(row.tech);
    ctx.hold_town_center = true;
    if building.idle && ctx.can_afford(cost) {
        claim(ctx, row, &building)?;
    } else {
        ctx.saving_castle = true;
        ctx.hold_military_training = true;
        ctx.reserve(cost);
    }
    Ok(true)
}

fn claim(ctx: &mut Context<'_>, row: &Technology, building: &Building) -> Done {
    ctx.spend(research_price(row.tech))?;
    ctx.draining.insert(building.id);
    if building.idle {
        let order = Order::Research {
            building: EntityId(building.id),
            tech: row.tech,
        };
        ctx.order_or_stop("research", order)?;
        let wait = ctx.config.research_retry_seconds * ctx.hz();
        ctx.board
            .cool(&format!("research:{:?}", row.tech), ctx.tick, wait);
    } else if land_producers().contains(&building.kind) {
        ctx.production_drain = true;
    }
    Ok(())
}

fn priority(row: &Technology, fielded: &Lines) -> (i64, i64, i64) {
    let beneficiaries: i64 = row
        .upgrades
        .iter()
        .map(|line| i64::from(fielded.get(*line)))
        .sum();
    let age = -(age_rank(row.age) as i64);
    if beneficiaries > 0 {
        return (0, -beneficiaries, age);
    }
    if row.unlocks.iter().any(|unit| profile(*unit).line == *unit) {
        return (1, 0, age);
    }
    if !row.upgrades.is_empty() || row.economic || ARMS_RESEARCH.contains(&row.building) {
        return (2, 0, age);
    }
    (3, 0, age)
}
