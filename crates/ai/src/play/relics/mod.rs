//! Use the same fog-filtered relic orders as a person playing the match.
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::dist2;
use crate::roster::price;
use crate::view::Tile;
use aoe_sdk::{BuildingKind, EntityId, Order, RelicAction, UnitKind};

/// Site the Monastery near a visible relic close to one of our villages.
pub(super) fn monastery_anchor(ctx: &Context<'_>) -> Option<Tile> {
    if !ctx.castle() {
        return None;
    }
    let range = collect_range_sq(ctx);
    ctx.world
        .relics()?
        .ground
        .iter()
        .flat_map(|relic| {
            let at = relic.position.tile_floor();
            let tile = (at.x, at.y);
            ctx.centers
                .iter()
                .map(move |&center| (dist2(center, tile), tile))
        })
        .filter(|(distance, _)| *distance <= range)
        .min()
        .map(|(_, tile)| tile)
}

fn collect_range_sq(ctx: &Context<'_>) -> i64 {
    i64::from(ctx.config.relic_collect_distance).pow(2)
}

pub fn step(ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
    if !ctx.castle() {
        return Ok(());
    }
    let world = ctx.world;
    let Some(view) = world.relics() else {
        return Ok(());
    };
    if view.ground.is_empty() && view.carriers.is_empty() {
        return Ok(());
    }
    let monasteries: Vec<_> = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Monastery && b.complete)
        .filter(|b| !view.monasteries.iter().any(|m| m.id.0 == b.id && m.full))
        .map(|b| {
            (
                b.id,
                b.center(),
                b.queue_length == 0 && b.researching.is_none(),
            )
        })
        .collect();
    if monasteries.is_empty() {
        return Ok(());
    }
    let monks: Vec<_> = ctx
        .units
        .iter()
        .filter(|u| u.kind == UnitKind::Monk && u.alive)
        .map(|u| {
            (
                EntityId(u.id),
                (u.x, u.y),
                u.idle && !ctx.busy.contains(&u.id),
            )
        })
        .collect();
    let range = collect_range_sq(ctx);
    let mut claimed = std::collections::BTreeSet::new();
    for &(id, from, _) in monks.iter().filter(|(_, _, idle)| *idle) {
        let action = if view.carriers.binary_search(&id).is_ok() {
            let mut by_distance: Vec<_> = monasteries
                .iter()
                .map(|&(id, at, _)| (dist2(at, from), id))
                .collect();
            by_distance.sort_unstable();
            let candidates: Vec<_> = by_distance
                .into_iter()
                .map(|(_, id)| EntityId(id))
                .collect();
            world
                .first_reachable_monastery(from, &candidates)
                .map(|building| Order::Garrison { unit: id, building })
        } else {
            let mut nearby: Vec<_> = view
                .ground
                .iter()
                .filter(|relic| !claimed.contains(&relic.id))
                .filter_map(|relic| {
                    let at = relic.position.tile_floor();
                    let tile = (at.x, at.y);
                    let distance = dist2(tile, from);
                    (distance <= range).then_some((distance, relic.id, tile))
                })
                .collect();
            nearby.sort_unstable();
            nearby
                .into_iter()
                .find(|&(_, _, tile)| tile == from || ctx.land_route(from, tile).is_some())
                .map(|(_, relic, _)| {
                    claimed.insert(relic);
                    Order::Relic(RelicAction::PickUp { monk: id, relic })
                })
        };
        if let Some(action) = action {
            ctx.order_or_stop("relic", action)?;
        }
    }
    // A completed Monastery with an opportunity and no existing collector
    // trains one Monk using the normal population and resource budget.
    if monks.is_empty() && ctx.free_population() > 0 {
        let train = monasteries
            .iter()
            .filter(|(_, _, ready)| *ready)
            .filter_map(|&(site, center, _)| {
                let near = view
                    .ground
                    .iter()
                    .map(|relic| {
                        let at = relic.position.tile_floor();
                        dist2((at.x, at.y), center)
                    })
                    .min()?;
                (near <= range).then_some((near, site))
            })
            .min();
        if let Some((_, site)) = train
            && crate::roster::can_recruit(ctx, UnitKind::Monk)
            && ctx.try_spend(price(UnitKind::Monk))?
        {
            ctx.order_or_stop(
                "train",
                Order::Train {
                    building: EntityId(site),
                    kind: UnitKind::Monk,
                },
            )?;
            ctx.queued += 1;
        }
    }
    Ok(())
}
