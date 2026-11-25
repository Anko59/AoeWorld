//! A small optional fleet, using only explored public river food and terrain
//! (`ai/aoe_ai/play/naval.py`), and the reach check of every food order.
use aoe_sdk::{BuildingKind, EntityId, Order, ResourceKind, UnitKind};

use super::constructor;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{dist, nearest};
use crate::roster::{best_recruit, options, price, visible_composition, warship};
use crate::view::{Building, Node, Unit};

type Done = Result<(), OrderBudgetExhausted>;

pub fn water_food(ctx: &Context<'_>, node: &Node) -> bool {
    node.kind == ResourceKind::Food && ctx.world.passable(node.tile()) == Some(false)
}

/// Fish are reached from an adjacent bank, never by a land route into water.
pub fn food_reachable(ctx: &mut Context<'_>, villager: &Unit, node: &Node) -> bool {
    if !water_food(ctx, node) {
        return ctx.land_route(villager.tile(), node.tile()).is_some();
    }
    let (x, y) = node.tile();
    let mut banks = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            let bank = (x + dx, y + dy);
            if (dx != 0 || dy != 0) && ctx.world.passable(bank) == Some(true) {
                banks.push(bank);
            }
        }
    }
    banks.sort_by(|a, b| dist(*a, villager.tile()).total_cmp(&dist(*b, villager.tile())));
    banks
        .into_iter()
        .any(|bank| ctx.land_route(villager.tile(), bank).is_some())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NavalManager {
    /// Python's manager keeps nothing: its state is on the blackboard.
    stateless: std::marker::PhantomData<()>,
}

impl NavalManager {
    pub fn step(&mut self, ctx: &mut Context<'_>) -> Done {
        let Some(home) = ctx.home else {
            return Ok(());
        };
        if ctx.economy.is_none() {
            return Ok(());
        }
        let reach = f64::from(ctx.config.max_gather_distance);
        let nodes = ctx.resources(ResourceKind::Food).to_vec();
        let fish: Vec<Node> = nodes
            .into_iter()
            .filter(|n| n.remaining > 0 && water_food(ctx, n) && dist(n.tile(), home) <= reach)
            .collect();
        if !fish.is_empty() {
            self.fish(ctx, &fish)?;
        }
        let dock = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Dock && b.complete)
            .min_by_key(|b| b.id)
            .cloned();
        let Some(dock) = dock else {
            if !fish.is_empty() && constructor::total(ctx, BuildingKind::Dock) == 0 {
                let Some(target) = nearest(&fish, home, Node::tile) else {
                    return Ok(());
                };
                if let Some(builder) = constructor::builder_near(ctx, target.tile()) {
                    let radius = Some(ctx.config.dock_search_radius);
                    constructor::place(
                        ctx,
                        BuildingKind::Dock,
                        target.tile(),
                        &[builder.id],
                        radius,
                        0,
                    )?;
                }
            }
            return Ok(());
        };
        if !dock.idle || ctx.draining.contains(&dock.id) || ctx.free_population() <= 0 {
            return Ok(());
        }
        let training = ctx.board.tracked.values().any(|t| {
            t.name == "train"
                && matches!(&t.order, Order::Train { building, .. } if building.0 == dock.id)
        });
        if training {
            return Ok(());
        }
        let key = format!("naval-train:{}", dock.id);
        if ctx.board.cooling(&key, ctx.tick) {
            return Ok(());
        }
        if let Some(kind) = Self::recruit(ctx, &dock, &fish)
            && ctx.try_spend(price(kind))?
        {
            let order = Order::Train {
                building: EntityId(dock.id),
                kind,
            };
            ctx.order_or_stop("train", order)?;
            ctx.queued += 1;
            let wait = ctx.config.naval_settle_seconds * ctx.hz();
            ctx.board.cool(&key, ctx.tick, wait);
        }
        Ok(())
    }

    fn recruit(ctx: &Context<'_>, dock: &Building, fish: &[Node]) -> Option<UnitKind> {
        let fishing = UnitKind::FishingShip;
        let ships = ctx
            .units
            .iter()
            .filter(|u| u.alive && u.kind == fishing)
            .count();
        if !fish.is_empty()
            && ships < ctx.config.fishing_ships as usize
            && ctx.can_afford(price(fishing))
            && !ctx.board.blocked_units.contains(&format!("{fishing:?}"))
        {
            return Some(fishing);
        }
        if !ctx.feudal() {
            return None;
        }
        let near: Vec<Unit> = ctx
            .enemies
            .iter()
            .filter(|e| dist(e.tile(), dock.center()) <= f64::from(ctx.config.fort_threat_radius))
            .cloned()
            .collect();
        let enemies = visible_composition(&near, warship);
        enemies.iter().next()?; // no warship near the dock: no navy
        let have = ctx.combat_lines().clone();
        let fleet: u32 = have
            .iter()
            .filter(|(kind, _)| warship(*kind))
            .map(|(_, n)| n)
            .sum();
        if fleet >= ctx.config.warships {
            return None;
        }
        let economy = ctx.economy.as_ref()?;
        let choices: Vec<UnitKind> = options(dock.kind, economy.age, &economy.researched)
            .into_iter()
            .filter(|kind| warship(*kind))
            .collect();
        best_recruit(ctx, &choices, &have, &enemies, None, None)
    }

    fn fish(&mut self, ctx: &mut Context<'_>, nodes: &[Node]) -> Done {
        let ships: Vec<Unit> = ctx
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::FishingShip && u.alive && u.idle)
            .cloned()
            .collect();
        for ship in ships {
            let key = format!("fishing:{}", ship.id);
            if ctx.busy.contains(&ship.id) || ctx.board.cooling(&key, ctx.tick) {
                continue;
            }
            if let Some(target) = nearest(nodes, ship.tile(), Node::tile) {
                let order = Order::Gather {
                    unit: EntityId(ship.id),
                    resource: target.id,
                };
                ctx.order_or_stop("gather", order)?;
                let wait = ctx.config.naval_settle_seconds * ctx.hz();
                ctx.board.cool(&key, ctx.tick, wait);
            }
        }
        Ok(())
    }
}
