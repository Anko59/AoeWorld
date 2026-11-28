//! Generals: their settings, and the orders that march their armies.
use std::collections::BTreeSet;

use aoe_sdk::{EntityId, GeneralState, Order, TileCoord, UnitKind};

use super::{Done, ManagementManager};
use crate::context::Context;
use crate::geometry::dist;
use crate::play::combat::CombatManager;
use crate::view::Unit;

fn formation_move(ctx: &mut Context<'_>, general: u32, to: (i32, i32)) -> Done {
    let order = Order::FormationMove {
        units: vec![EntityId(general)],
        formation: ctx.config.army_formation,
        to: TileCoord::new(to.0, to.1),
        queued: false,
        attack_move: true,
    };
    ctx.order_or_stop("formation_move", order).map(|_| ())
}

impl ManagementManager {
    pub fn generals(&mut self, ctx: &mut Context<'_>) -> Done {
        let cfg = ctx.config;
        let generals: Vec<Unit> = ctx
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::General && u.alive)
            .cloned()
            .collect();
        for general in generals {
            let enabled = cfg.general_train
                && !(ctx.saving_castle || ctx.production_drain)
                && crate::roster::has_army_supplies(ctx, 0);
            if !ctx.board.generals.contains(&general.id) {
                let order = Order::GeneralMix {
                    unit: EntityId(general.id),
                    mix: cfg.general_mix,
                };
                ctx.order_or_stop("general_mix", order)?;
                ctx.board.generals.insert(general.id);
            }
            let key = format!("gtrain:{}", general.id);
            let refill = enabled
                && general
                    .army_size
                    .is_some_and(|size| u32::from(size) < cfg.general_refill_below)
                && !ctx.board.cooling(&key, ctx.tick);
            if ctx.board.general_training.get(&general.id) != Some(&enabled) || refill {
                let order = Order::GeneralTrain {
                    unit: EntityId(general.id),
                    enabled,
                };
                ctx.order_or_stop("general_train", order)?;
                ctx.board.general_training.insert(general.id, enabled);
                let wait = 30 * ctx.hz();
                ctx.board.cool(&key, ctx.tick, wait);
            }
        }
        // `army_size` only counts the commanded soldiers; the General seat comes first.
        let led: u64 = ctx
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::General)
            .map(|u| u64::from(u.army_size.unwrap_or(0)))
            .sum();
        let peak = ctx.board.stats.entry("led_peak".to_owned()).or_insert(0);
        *peak = (*peak).max(led);
        Ok(())
    }

    /// March each General's army at the nearest known enemy building.
    pub fn lead(&mut self, ctx: &mut Context<'_>, combat: &CombatManager) -> Done {
        let cfg = ctx.config;
        let Some(home) = ctx.home else {
            return Ok(());
        };
        // A General whose soldiers are fighting stays with them: moving him
        // would drag the army out of the fight it is in.
        let fighting: BTreeSet<u32> = ctx
            .led
            .iter()
            .filter(|u| u.attack_target.is_some())
            .filter_map(|u| u.commanded_by)
            .collect();
        let generals: Vec<Unit> = ctx
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::General && u.alive)
            .cloned()
            .collect();
        let hop_tiles = f64::from(cfg.march_hop_tiles);
        for general in generals {
            if fighting.contains(&general.id) {
                continue;
            }
            let at = general.tile();
            if ctx.board.raid_until > ctx.tick as i64 {
                let Some(target) = ctx.board.raid_tile else {
                    continue;
                };
                let key = format!("recall-general:{}", general.id);
                if dist(at, target) > f64::from(cfg.defend_radius / 2)
                    && !ctx.board.cooling(&key, ctx.tick)
                {
                    let hop = ctx.land_hop(at, target);
                    if hop != at {
                        formation_move(ctx, general.id, hop)?;
                        let wait = cfg.raid_general_retry_seconds * ctx.hz();
                        ctx.board.cool(&key, ctx.tick, wait);
                    }
                }
                continue;
            }
            if ctx.busy.contains(&general.id)
                || general.general_state != Some(GeneralState::Idle)
                || !general.idle
            {
                continue;
            }
            let led = u32::from(general.army_size.unwrap_or(0));
            if led < cfg.general_attack_army {
                if led < cfg.general_retreat_army && dist(at, home) > 14.0 {
                    let hop = ctx.land_hop(at, home);
                    let order = Order::Move {
                        unit: EntityId(general.id),
                        to: TileCoord::new(hop.0, hop.1),
                    };
                    ctx.order_or_stop("move", order)?;
                }
                continue;
            }
            let goal = match combat.target_building(ctx, at, None) {
                Some(building) => {
                    if dist(at, building.center()) <= hop_tiles && building.visible {
                        building.center()
                    } else {
                        combat.approach(ctx, &building, at)
                    }
                }
                None => {
                    let (width, height) = ctx.world.map_size();
                    let far = (width - 1 - home.0, height - 1 - home.1);
                    if dist(at, far) < 15.0 {
                        (width / 2, height / 2)
                    } else {
                        far
                    }
                }
            };
            let hop = if dist(at, goal) > hop_tiles {
                ctx.land_hop(at, goal)
            } else {
                goal
            };
            if hop == at {
                continue;
            }
            let hop = ctx.unrefused(hop);
            formation_move(ctx, general.id, hop)?;
            ctx.board.bump("general_marches");
        }
        Ok(())
    }
}
