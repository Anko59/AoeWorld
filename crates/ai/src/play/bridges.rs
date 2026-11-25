//! Optional local crossing heuristic, using only explored public terrain
//! (`ai/aoe_ai/play/bridges.py`).
use aoe_sdk::{Age, BuildingKind, EntityId, Order, TileCoord, UnitTask};

use crate::blackboard::Plan;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{dist, nearest};
use crate::roster::building_price;
use crate::view::Unit;

#[derive(Clone, Copy, Debug, Default)]
pub struct BridgeManager {
    /// Python's manager keeps nothing: its state is on the blackboard.
    stateless: std::marker::PhantomData<()>,
}

impl BridgeManager {
    pub fn step(&self, ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
        let feudal = ctx.economy.as_ref().is_some_and(|e| e.age == Age::Feudal);
        if !ctx.config.build_bridges || !feudal || ctx.army.is_empty() {
            return Ok(());
        }
        if ctx.board.cooling("bridge", ctx.tick) {
            return Ok(());
        }
        let wait = ctx.config.bridge_retry_seconds * ctx.hz();
        ctx.board.cool("bridge", ctx.tick, wait);
        if ctx
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Bridge && !b.complete)
        {
            return Ok(());
        }
        let soldier = ctx.army[0].tile();
        let mut targets = ctx.world.known_enemy_buildings();
        targets.sort_by(|a, b| {
            dist(soldier, a.center())
                .total_cmp(&dist(soldier, b.center()))
                .then(a.id.cmp(&b.id))
        });
        let Some(nearest_target) = targets.first() else {
            return Ok(());
        };
        let target = nearest_target.center();
        if ctx.land_route(soldier, target).is_some() {
            return Ok(());
        }
        let Some(bank) = ctx.land_frontier(soldier, target) else {
            return Ok(());
        };
        let Some((start, end)) = ctx.world.find_bridge_site(bank, target) else {
            return Ok(());
        };
        let workers: Vec<Unit> = ctx
            .villagers
            .iter()
            .filter(|v| {
                !ctx.busy.contains(&v.id)
                    && !ctx.board.builders.contains_key(&v.id)
                    && v.task != UnitTask::Farming
            })
            .cloned()
            .collect();
        let Some(worker) = nearest(&workers, start, Unit::tile) else {
            return Ok(());
        };
        if ctx.land_route(worker.tile(), start).is_none() {
            return Ok(());
        }
        let count = (end.0 - start.0).abs().max((end.1 - start.1).abs()) - 1;
        let cost = building_price(BuildingKind::Bridge).map(|n| n * count.max(0) as u32);
        if !ctx.can_afford(cost) {
            return Ok(());
        }
        ctx.spend(cost)?;
        let order = Order::BuildBridge {
            builders: vec![EntityId(worker.id)],
            from: TileCoord::new(start.0, start.1),
            to: TileCoord::new(end.0, end.1),
        };
        ctx.order_or_stop("build_bridge", order)?;
        let (tick, id) = (ctx.tick, worker.id);
        let board = &mut *ctx.board;
        board.plans.push(Plan {
            kind: BuildingKind::Bridge,
            tile: (start.0.min(end.0), start.1.min(end.1)),
            tick,
        });
        board
            .builders
            .insert(id, (format!("{:?}", BuildingKind::Bridge), start));
        board.bump("bridges");
        Ok(())
    }
}
