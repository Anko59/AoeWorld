//! Bounded, rotating Town Center production across the living kingdom
//! (`ai/aoe_ai/play/production.py`).
use aoe_sdk::{BuildingKind, EntityId, Order, RallyTarget, ResourceKind, UnitKind};

use super::buildorder;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::dist2;
use crate::roster::price;
use crate::view::Node;

pub fn train_villagers(ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
    let hz = ctx.hz();
    if ctx.hold_town_center || ctx.board.cooling("world-limit:train", ctx.tick) {
        return Ok(());
    }
    if ctx.tick as i64 - ctx.board.hold_villagers_tick <= 3 * hz as i64 {
        return Ok(());
    }
    let centers: Vec<_> = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::TownCenter && b.complete)
        .cloned()
        .collect();
    if centers.is_empty() {
        return Ok(());
    }
    let mut queued: usize = centers.iter().map(|c| c.queue_length).sum();
    let target = buildorder::villager_target(ctx);
    if ctx.villagers.len() + queued >= target {
        return Ok(());
    }
    let start = ctx.board.production_cursor % centers.len();
    ctx.board.production_cursor = (start + 1) % centers.len();
    let reach = i64::from(ctx.config.max_gather_distance).pow(2);
    for center in centers[start..].iter().chain(&centers[..start]) {
        if center.researching.is_some() || ctx.draining.contains(&center.id) {
            continue;
        }
        // Each domain gets its own nearby rally. Managed domains choose jobs
        // themselves; a remote first-TC rally must not drag their workers home.
        if center.garrison.is_empty() {
            for kind in [ResourceKind::Food, ResourceKind::Wood] {
                let origin = center.center();
                let mut nodes: Vec<Node> = ctx.resources(kind).to_vec();
                nodes.sort_by_key(|n| dist2(n.tile(), origin));
                nodes.truncate(8);
                let mut found = None;
                for node in nodes {
                    if dist2(node.tile(), origin) <= reach
                        && ctx.land_route(origin, node.tile()).is_some()
                    {
                        found = Some(node);
                        break;
                    }
                }
                if let Some(node) = found {
                    if center.rally_resource != Some(node.id) {
                        let order = Order::SetRally {
                            building: EntityId(center.id),
                            target: RallyTarget::Resource(node.id),
                        };
                        ctx.order_or_stop("set_rally_resource", order)?;
                    }
                    break;
                }
            }
        }
        let mut queue = center.queue_length;
        while queue < ctx.config.town_center_queue as usize
            && ctx.free_population() > 0
            && ctx.villagers.len() + queued < target
            && ctx.try_spend(price(UnitKind::Villager))?
        {
            let order = Order::Train {
                building: EntityId(center.id),
                kind: UnitKind::Villager,
            };
            ctx.order_or_stop("train", order)?;
            ctx.queued += 1;
            queue += 1;
            queued += 1;
            ctx.board.bump("villagers_queued");
        }
    }
    Ok(())
}
