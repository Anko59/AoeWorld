use aoe_sdk::{
    AllianceOffer, BuildingKind, CommandResult, DiplomacyEntry, DiplomacyMessage, DiplomaticStance,
    EntityId, Order, PlayerId, TileCoord,
};

use crate::{
    blackboard::{DiplomacyProgress, slot},
    context::Context,
    geometry::dist,
};

pub(super) fn step(ctx: &mut Context<'_>) {
    if let Some(view) = ctx.world.diplomacy() {
        for offer in view
            .seats
            .iter()
            .filter(|seat| seat.offer == AllianceOffer::Received)
        {
            if ctx
                .order(
                    "accept_alliance",
                    Order::Diplomacy {
                        player: offer.player,
                        stance: DiplomaticStance::Ally,
                    },
                )
                .is_none()
            {
                return;
            }
        }
        ctx.board
            .diplomacy_replies
            .retain(|id, _| view.log.iter().any(|entry| entry.id == *id));
        for request in view.log.iter().filter(|entry| entry.to == view.viewer) {
            if view.log.iter().any(|entry| {
                matches!(entry.message, DiplomacyMessage::Reply { request: id, .. } if id == request.id)
            }) {
                continue;
            }
            answer(
                ctx,
                *request,
                view.stance(request.from) == DiplomaticStance::Ally,
            );
        }
    }
    seek_support(ctx);
}

fn answer(ctx: &mut Context<'_>, request: DiplomacyEntry, allied: bool) {
    use DiplomacyMessage::*;
    use DiplomacyProgress::*;
    let accepted = match ctx.board.diplomacy_replies.get(&request.id) {
        Some(Answered | Mission(_)) => return,
        Some(Ready(accepted)) => *accepted,
        None => match request.message {
            RequestStance(stance) => stance != DiplomaticStance::Enemy,
            RequestTribute { resource, amount } => allied && can_spare(ctx, resource, amount),
            AttackPlayer(_) | Defend(_) => {
                if ctx.orders == ctx.order_limit {
                    return;
                }
                let mission = match request.message {
                    AttackPlayer(target) if allied => attack(ctx, target),
                    Defend(at) if allied => march(ctx, at),
                    _ => None,
                };
                if let Some(sequence) = mission {
                    ctx.board
                        .diplomacy_replies
                        .insert(request.id, Mission(sequence));
                    return;
                }
                false
            }
            Reply { .. } | Surrender => return,
        },
    };
    if ctx
        .order(
            "answer_diplomacy",
            Order::DiplomacyMessage {
                player: request.from,
                message: Reply {
                    request: request.id,
                    accepted,
                },
            },
        )
        .is_some()
    {
        ctx.board.diplomacy_replies.insert(request.id, Answered);
    }
}

fn can_spare(ctx: &Context<'_>, resource: aoe_sdk::ResourceKind, amount: u32) -> bool {
    let Some(bank) = &ctx.economy else {
        return false;
    };
    let reserves = [
        ctx.config.market_food_reserve,
        ctx.config.market_wood_reserve,
        ctx.config.market_gold_reserve,
        ctx.config.market_stone_reserve,
    ];
    let at = slot(resource);
    let available = [bank.food, bank.wood, bank.gold, bank.stone][at];
    let fee = if bank.has_researched(aoe_sdk::Tech::Banking) {
        0
    } else if bank.has_researched(aoe_sdk::Tech::Coinage) {
        20
    } else {
        30
    };
    ctx.buildings
        .iter()
        .any(|b| b.complete && b.kind == BuildingKind::Market)
        && (u64::from(amount) * (100 + fee)).div_ceil(100)
            <= u64::from(available.saturating_sub(reserves[at]))
}

fn attack(ctx: &mut Context<'_>, target: PlayerId) -> Option<u64> {
    let at = ctx
        .enemies
        .iter()
        .find(|unit| unit.alive && unit.owner == target.0)
        .map(|u| u.tile())
        .or_else(|| {
            ctx.world
                .known_enemy_buildings()
                .into_iter()
                .find(|b| b.hp > 0 && b.owner == target.0)
                .map(|b| b.center())
        });
    at.and_then(|(x, y)| march(ctx, TileCoord::new(x, y)))
}

fn march(ctx: &mut Context<'_>, at: TileCoord) -> Option<u64> {
    let units = ctx
        .army
        .iter()
        .filter(|unit| unit.alive && unit.idle && !ctx.busy.contains(&unit.id))
        .take(ctx.config.diplomacy_detachment_size)
        .map(|unit| EntityId(unit.id))
        .collect::<Vec<_>>();
    if units.is_empty() {
        return None;
    }
    ctx.order(
        "allied_mission",
        Order::FormationMove {
            units,
            formation: ctx.config.army_formation,
            to: at,
            queued: false,
            attack_move: true,
        },
    )
}

fn seek_support(ctx: &mut Context<'_>) {
    let Some(home) = ctx.home else { return };
    if ctx.board.cooling("seek_support", ctx.tick) {
        return;
    }
    let own = ctx.world.me().map(PlayerId);
    let near = |unit: &&crate::view::Unit| {
        unit.alive && unit.military && dist(unit.tile(), home) <= ctx.config.diplomacy_threat_radius
    };
    // A local threat gives us a reason to seek an ally among factions we
    // have actually encountered. The server checks recent combat.
    let Some(threat) = ctx
        .enemies
        .iter()
        .filter(near)
        .find(|unit| unit.attack_target.is_some())
    else {
        return;
    };
    let seats = ctx
        .world
        .diplomacy()
        .into_iter()
        .flat_map(|view| &view.seats);
    let ally = seats
        .clone()
        .find(|seat| seat.stance == DiplomaticStance::Ally);
    let order = if let Some(ally) = ally {
        Some(Order::DiplomacyMessage {
            player: ally.player,
            message: DiplomacyMessage::Defend(TileCoord::new(home.0, home.1)),
        })
    } else {
        seats
            .filter(|seat| seat.stance == DiplomaticStance::Neutral)
            .map(|seat| seat.player)
            .chain(
                ctx.enemies
                    .iter()
                    .filter(|unit| unit.alive && unit.attack_target.is_none())
                    .map(|unit| PlayerId(unit.owner)),
            )
            .filter(|&player| Some(player) != own && player.0 != threat.owner)
            .min()
            .map(|player| Order::Diplomacy {
                player,
                stance: DiplomaticStance::Ally,
            })
    };
    if let Some(order) = order
        && ctx.order("seek_support", order).is_some()
    {
        ctx.board.cool(
            "seek_support",
            ctx.tick,
            ctx.config.diplomacy_offer_interval_seconds * u64::from(ctx.world.tick_hz()),
        );
    }
}

pub(super) fn mission_result(ctx: &mut Context<'_>, sequence: u64, result: CommandResult) {
    for progress in ctx.board.diplomacy_replies.values_mut() {
        if *progress == DiplomacyProgress::Mission(sequence) {
            *progress = DiplomacyProgress::Ready(result == CommandResult::Accepted);
            return;
        }
    }
}

pub(super) fn refused(ctx: &mut Context<'_>, request: u64, accepted: bool) {
    if accepted && let Some(progress) = ctx.board.diplomacy_replies.get_mut(&request) {
        *progress = DiplomacyProgress::Ready(false);
    }
}
