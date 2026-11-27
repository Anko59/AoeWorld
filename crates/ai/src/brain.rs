//! The player: one `Brain` per slot, called at every decision point
//! (`ai/aoe_ai/brain.py`). It learns from the server's verdicts and keeps the
//! peak statistics; the managers run after that, in priority order.
use aoe_sdk::{Age, BuildingKind, CommandResult, EntityId, Event, Order};

use crate::blackboard::{Blackboard, Tracked};
use crate::config::Config;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::dist;
use crate::play::bridges::BridgeManager;
use crate::play::combat::CombatManager;
use crate::play::economy::{self, unemployed};
use crate::play::fortification::FortificationManager;
use crate::play::fortification::threat::observe;
use crate::play::management::ManagementManager;
use crate::play::naval::NavalManager;
use crate::play::scouting::ScoutManager;
use crate::play::{buildorder, constructor, market, military, research};
use crate::roster::profile;
use crate::seat::Seat;
use crate::view::{Tile, View};
use crate::worker_view::survey_workers;

/// Refusals that mean "this target will never work for this unit".
fn hopeless(result: CommandResult) -> bool {
    matches!(
        result,
        CommandResult::RejectedInvalidResource
            | CommandResult::RejectedWrongUnitKind
            | CommandResult::RejectedUnreachable
            | CommandResult::RejectedResourceUnavailable
            | CommandResult::RejectedUnknownEntity
    )
}

pub struct Brain {
    pub config: Config,
    pub board: Blackboard,
}

impl Brain {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            board: Blackboard::default(),
        }
    }

    /// One decision: learn from the events, then run the managers.
    pub fn decide(&mut self, world: &dyn View, events: &[Event], seat: &mut dyn Seat) {
        let mut ctx = Context::new(world, events, &mut self.board, &self.config, seat);
        absorb(&mut ctx);
        note_damage(&mut ctx);
        if ctx.castle() {
            let castle_units = ctx
                .army
                .iter()
                .chain(&ctx.led)
                .filter(|u| u.alive && profile(u.kind).age == Age::Castle)
                .count();
            peak(ctx.board, "castle_units_peak", castle_units);
        }
        let army = ctx.army.len() + ctx.led.len();
        peak(ctx.board, "army_peak", army);
        let population = ctx.economy.as_ref().map_or(0, |e| e.population);
        peak(ctx.board, "pop_peak", population as usize);
        let villagers = ctx.villagers.len();
        peak(ctx.board, "villagers_peak", villagers);
        if ctx.economy.is_none() {
            return;
        }
        run_managers(&mut ctx);
        ctx.board.bump("decisions");
    }
}

/// Raise the statistic `name` to `value` when it is a new peak.
fn peak(board: &mut Blackboard, name: &str, value: usize) {
    let entry = board.stats.entry(name.to_owned()).or_insert(0);
    *entry = (*entry).max(value as u64);
}

/// Run one manager with its slice of the order budget. Idle stages leave room
/// for later work; a manager that spends its slice ends its work there
/// (`OrderBudgetExhausted`).
fn run(
    ctx: &mut Context<'_>,
    limit: u32,
    action: impl FnOnce(&mut Context<'_>) -> Result<(), OrderBudgetExhausted>,
) {
    let total = ctx.config.orders_per_decision;
    if ctx.orders >= total {
        return;
    }
    ctx.order_limit = total.min(ctx.orders + limit);
    let _ = action(ctx);
}

/// The managers in priority order, each with its slice of the order budget
/// (`brain.py` `decide`).
fn run_managers(ctx: &mut Context<'_>) {
    let config = ctx.config;
    if ctx.late_economy && ctx.villagers.iter().any(unemployed) {
        ctx.land_query_limit -= config.idle_land_query_reserve.min(ctx.land_query_limit / 2);
    }
    constructor::refresh(ctx);
    if ctx.center.is_some()
        && buildorder::imperial_ready(ctx)
        && buildorder::imperial_requirement_kinds(ctx) < 2
    {
        run(ctx, 1, |c| constructor::production(c, None));
        let wood = ctx.economy.as_ref().map_or(0, |e| e.wood);
        let free = wood.saturating_sub(ctx.spent[1]);
        ctx.spent[1] += buildorder::imperial_prerequisite_wood(ctx).min(free);
    }
    run(ctx, 2, research::step);
    run(ctx, 1, market::step);
    run(ctx, 14, |c| CombatManager::default().step(c));
    run(ctx, 2, |c| {
        ManagementManager::default().lead(c, &CombatManager::default())
    });
    run(ctx, 4, crate::play::relics::step);
    run(ctx, 2, military::rally);
    run(ctx, 2, |c| ScoutManager::default().step(c));
    run(ctx, 1, constructor::fund_market);
    run(ctx, 1, economy::wood_dropoff);
    run(ctx, 8, |c| ManagementManager::default().step(c));
    run(ctx, 5, economy::produce);
    run(ctx, 2, constructor::houses);
    run(ctx, 1, |c| FortificationManager::default().step(c));
    if buildorder::unpaid_age_up_barracks(ctx)
        || (ctx.feudal() && !ctx.castle() && buildorder::castle_requirement_kinds(ctx) < 2)
    {
        run(ctx, 1, |c| constructor::production(c, None));
        ctx.spent[1] += buildorder::castle_prerequisite_wood(ctx);
    }
    let forming_army = buildorder::wants_castle_food(ctx)
        && buildorder::stat(ctx, "army_peak") < u64::from(config.castle_army_min);
    if forming_army {
        run(ctx, 4, military::train);
    }
    ctx.land_query_limit = config.land_queries_per_decision;
    run(ctx, 18, economy::work);
    survey_workers(ctx);
    run(ctx, 2, constructor::step);
    if !forming_army {
        let train_orders = if ctx.castle() {
            config.army_train_orders
        } else {
            4
        };
        run(ctx, train_orders, military::train);
    }
    run(ctx, 1, |c| NavalManager::default().step(c));
    run(ctx, 1, |c| BridgeManager::default().step(c));
}

/// Count a blow of ours landing: an enemy unit or building whose hit points
/// fell since we last saw it, with a soldier of ours in range. A foundation
/// rising to full health, or a wound given by someone else far from our army,
/// is not one.
pub fn note_damage(ctx: &mut Context<'_>) {
    let mut seen: Vec<(u32, u16, Tile)> =
        ctx.enemies.iter().map(|e| (e.id, e.hp, e.tile())).collect();
    for building in ctx.world.known_enemy_buildings() {
        let entry = (building.id, building.hp, building.center());
        match seen.iter_mut().find(|(id, ..)| *id == building.id) {
            Some(known) => *known = entry,
            None => seen.push(entry),
        }
    }
    let reach = f64::from(ctx.config.hit_reach);
    let board = &mut *ctx.board;
    for &(id, hp, place) in &seen {
        let before = board.enemy_hp.insert(id, hp);
        if before.is_none_or(|before| hp >= before) {
            continue;
        }
        if ctx.army.iter().any(|u| dist(u.tile(), place) <= reach) {
            board.wounded.insert(id);
            board.bump("enemies_wounded");
            if board.attacking {
                board.bump("attack_hits");
            }
        }
    }
    if board.enemy_hp.len() > 4096 {
        board
            .enemy_hp
            .retain(|id, _| seen.iter().any(|(seen_id, ..)| seen_id == id));
    }
}

// ----- learning from the server's verdicts ----------------------------------
pub fn absorb(ctx: &mut Context<'_>) {
    observe(ctx);
    let events = ctx.events;
    for event in events {
        match event {
            Event::OrderResult { sequence, result } => {
                let tracked = ctx.board.tracked.remove(sequence);
                if *result == CommandResult::Accepted {
                    ctx.board.bump("accepted");
                    continue;
                }
                ctx.board.bump("rejected");
                ctx.board.bump(&format!("rejected:{result:?}"));
                if let Some(tracked) = tracked {
                    ctx.board
                        .bump(&format!("rejected:{}:{result:?}", tracked.name));
                    on_rejected(ctx, &tracked, *result);
                }
            }
            Event::UnitDied { mine: true, .. } => ctx.board.bump("units_lost"),
            Event::UnitDied { .. } => ctx.board.bump("kills"),
            Event::BuildingCompleted { .. } => ctx.board.bump("buildings_completed"),
            Event::AgeAdvanced { .. } => {
                ctx.board.bump("ages");
                let key = if ctx.imperial() {
                    "imperial_tick"
                } else if ctx.castle() {
                    "castle_tick"
                } else {
                    "feudal_tick"
                };
                ctx.board.stats.entry(key.to_owned()).or_insert(ctx.tick);
                ctx.board.blocked_units.clear();
            }
            _ => {}
        }
    }
}

/// What the brain learns from a refusal of a tracked order.
pub fn on_rejected(ctx: &mut Context<'_>, tracked: &Tracked, result: CommandResult) {
    let (tick, hz) = (ctx.tick, u64::from(ctx.world.tick_hz()));
    let config = ctx.config;
    let mem = &mut *ctx.board;
    let name = tracked.name.as_str();
    match &tracked.order {
        Order::StewardSettings { unit, .. } | Order::VassalDevelopment { unit, .. } => {
            mem.manager_settings.remove(&unit.0);
        }
        Order::GeneralTrain { unit, .. } => {
            mem.general_training.remove(&unit.0);
        }
        _ => {}
    }
    if result == CommandResult::RejectedWorldLimit {
        // The world is full, not the site or target: wait, then try again.
        mem.cool(
            &format!("world-limit:{name}"),
            tick,
            config.world_limit_wait_seconds * hz,
        );
        match &tracked.order {
            Order::Build {
                builders, origin, ..
            } => release_plan(mem, builders, (origin.x, origin.y), None),
            Order::BuildBridge {
                builders, from, to, ..
            } => release_plan(mem, builders, (from.x.min(to.x), from.y.min(to.y)), None),
            _ => {}
        }
        return;
    }
    match &tracked.order {
        Order::Gather { resource, .. } if hopeless(result) => {
            mem.bad_resources.insert(*resource);
        }
        Order::Build {
            builders,
            kind,
            origin,
        } => {
            // Exploration can make this site legal later. Keep permanent
            // refusals for blocked or unreachable ground only.
            if result != CommandResult::RejectedUnexploredPlacement {
                mem.bad_sites
                    .insert((format!("{kind:?}"), origin.x, origin.y));
            }
            release_plan(mem, builders, (origin.x, origin.y), None);
        }
        Order::BuildBridge {
            builders, from, to, ..
        } => {
            let start = (from.x.min(to.x), from.y.min(to.y));
            release_plan(mem, builders, start, Some(BuildingKind::Bridge));
        }
        Order::Move { unit, to } if refused_destination(result) => {
            forget_destination(mem, (to.x, to.y), &[unit.0]);
        }
        Order::FormationMove { units, to, .. } if refused_destination(result) => {
            let units: Vec<u32> = units.iter().map(|u| u.0).collect();
            forget_destination(mem, (to.x, to.y), &units);
        }
        Order::Hunt { animal, .. } if hopeless(result) => {
            mem.bad_resources.insert(u64::from(animal.0));
        }
        Order::Attack { unit, target }
            if hopeless(result)
                || matches!(
                    result,
                    CommandResult::RejectedInvalidTarget
                        | CommandResult::RejectedPathBudgetExceeded
                ) =>
        {
            let wait = config.attack_refusal_wait_seconds * hz;
            mem.cool(&format!("attack:{}:{}", unit.0, target.0), tick, wait);
        }
        Order::Farm { unit, farm } => {
            if mem.farmers.get(&farm.0) == Some(&unit.0) {
                mem.farmers.remove(&farm.0);
            }
            let wait = config.farm_refusal_wait_seconds * hz;
            mem.cool(&format!("farm:{}", farm.0), tick, wait);
        }
        Order::Train { kind, .. } if result == CommandResult::RejectedRequiresFeudalAge => {
            mem.blocked_units.insert(format!("{kind:?}"));
        }
        Order::Research { tech, .. } => {
            mem.cool(&format!("research:{tech:?}"), tick, 8 * hz);
        }
        _ => {}
    }
}

fn refused_destination(result: CommandResult) -> bool {
    matches!(
        result,
        CommandResult::RejectedInvalidDestination | CommandResult::RejectedUnreachable
    )
}

/// Forget the plan at `tile` (only a plan of `kind` when given) and free its builders.
fn release_plan(
    mem: &mut Blackboard,
    builders: &[EntityId],
    tile: Tile,
    kind: Option<BuildingKind>,
) {
    mem.plans.retain(|p| match kind {
        Some(kind) => p.kind != kind || p.tile != tile,
        None => p.tile != tile,
    });
    for builder in builders {
        mem.builders.remove(&builder.0);
    }
}

/// Remember a refused destination; an explicit refusal permits a different
/// hop for those soldiers immediately.
fn forget_destination(mem: &mut Blackboard, tile: Tile, units: &[u32]) {
    if mem.bad_moves.len() >= 4096 {
        mem.bad_moves.clear(); // bounded: forget the oldest lessons at once
    }
    mem.bad_moves.insert(tile);
    for unit in units {
        mem.marched.remove(unit);
    }
}
