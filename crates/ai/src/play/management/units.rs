//! Stewards and Vassals: seating, recruiting, saving for their price, their
//! settings and the workers a Vassal keeps.
use std::collections::BTreeSet;

use aoe_sdk::rules::unit_train_seconds;
use aoe_sdk::{BuildingKind, EntityId, Order, TileCoord, UnitKind, UnitTask};

use super::{Done, ManagementManager, available_vassals, garrisoned, ready_for_stewards};
use crate::context::Context;
use crate::geometry::{dist, dist2};
use crate::play::buildorder::{self, Stage};
use crate::play::economy::shares;
use crate::play::fortification::threat::threatened;
use crate::roster::price;
use crate::view::{Building, Unit};

/// `str(kind).split('.')[-1].lower()`
fn lower(kind: UnitKind) -> String {
    format!("{kind:?}").to_lowercase()
}

impl ManagementManager {
    /// Walk every idle `kind` into a completed `house` that has none.
    pub fn seat(&mut self, ctx: &mut Context<'_>, kind: UnitKind, house: BuildingKind) -> Done {
        let inside: BTreeSet<u32> = ctx
            .buildings
            .iter()
            .flat_map(|b| b.garrison.iter().map(|entry| entry.0))
            .collect();
        let mut seats: Vec<Building> = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == house && b.complete && !garrisoned(b, kind))
            .cloned()
            .collect();
        if kind == UnitKind::Vassal {
            seats.retain(|b| threatened(ctx, b.center()));
        }
        let mut taken: BTreeSet<u32> = ctx
            .board
            .seated
            .iter()
            .filter(|(unit, _)| ctx.units.iter().any(|x| x.id == **unit))
            .map(|(_, target)| *target)
            .collect();
        let units: Vec<Unit> = ctx.units.clone();
        for unit in units {
            if unit.kind != kind
                || !unit.alive
                || ctx.busy.contains(&unit.id)
                || inside.contains(&unit.id)
                || !unit.idle
            {
                continue;
            }
            let mut sorted: Vec<&Building> = seats.iter().collect();
            sorted.sort_by_key(|b| dist2(b.center(), unit.tile()));
            let target = sorted
                .into_iter()
                .find(|b| !taken.contains(&b.id) || ctx.board.seated.get(&unit.id) == Some(&b.id));
            let key = format!("seat:{}", unit.id);
            let Some(target) = target else { continue };
            if ctx.board.cooling(&key, ctx.tick) {
                continue;
            }
            taken.insert(target.id);
            ctx.board.seated.insert(unit.id, target.id);
            let order = Order::Garrison {
                unit: EntityId(unit.id),
                building: EntityId(target.id),
            };
            ctx.order_or_stop("garrison", order)?;
            let wait = 15 * ctx.hz();
            ctx.board.cool(&key, ctx.tick, wait);
            ctx.board.bump(&format!("seated_{}", lower(kind)));
        }
        Ok(())
    }

    pub fn recruit(&mut self, ctx: &mut Context<'_>) -> Done {
        let cfg = ctx.config;
        if !ctx.castle() || (ctx.hold_town_center && ctx.saving_castle) {
            return Ok(());
        }
        // Stewards: one per Town Center, from enough villagers.
        if ready_for_stewards(ctx) {
            let have =
                Self::fielded(ctx, UnitKind::Steward) + Self::in_production(ctx, UnitKind::Steward);
            let wanted = (cfg.stewards as usize).max(ctx.villagers.len().div_ceil(80));
            if have < wanted.min(ctx.count(BuildingKind::TownCenter, true)) {
                self.train(ctx, BuildingKind::TownCenter, UnitKind::Steward, |_, b| {
                    !garrisoned(b, UnitKind::Steward)
                })?;
            }
        }
        if ctx.count(BuildingKind::Castle, true) == 0 {
            return Ok(());
        }
        let vassals = available_vassals(ctx);
        let queued = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Castle && b.queue_length > 0)
            .count();
        let frontier_castles = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Castle && b.complete && threatened(ctx, b.center()))
            .count();
        let generals = ctx
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::General)
            .count();
        let castles = ctx.count(BuildingKind::Castle, true);
        if cfg.vassals > 0 && vassals + queued < frontier_castles {
            self.train(ctx, BuildingKind::Castle, UnitKind::Vassal, |ctx, b| {
                threatened(ctx, b.center()) && !garrisoned(b, UnitKind::Vassal)
            })?;
        } else if cfg.generals > 0 && generals + queued < (cfg.generals as usize).max(castles) {
            self.train(ctx, BuildingKind::Castle, UnitKind::General, |_, _| true)?;
        }
        Ok(())
    }

    /// Units of `kind` that exist: in the field or sheltered in a building.
    fn fielded(ctx: &Context<'_>, kind: UnitKind) -> usize {
        let fielded: BTreeSet<u32> = ctx
            .units
            .iter()
            .filter(|u| u.kind == kind)
            .map(|u| u.id)
            .collect();
        fielded.len()
            + ctx
                .buildings
                .iter()
                .flat_map(|b| &b.garrison)
                .filter(|entry| entry.1 == kind && !fielded.contains(&entry.0))
                .count()
    }

    /// Units ordered and still training: counted until one more exists or the
    /// training time (plus a margin for a refused order) has passed.
    fn in_production(ctx: &mut Context<'_>, kind: UnitKind) -> usize {
        let now = ctx.tick;
        let existing = Self::fielded(ctx, kind);
        let window = (u64::from(unit_train_seconds(kind)) + 20) * ctx.hz();
        let key = format!("{kind:?}");
        let live: Vec<(u64, usize)> = ctx
            .board
            .producing
            .get(&key)
            .into_iter()
            .flatten()
            .filter(|(t, before)| now.saturating_sub(*t) < window && existing <= *before)
            .copied()
            .collect();
        let count = live.len();
        ctx.board.producing.insert(key, live);
        count
    }

    fn train(
        &mut self,
        ctx: &mut Context<'_>,
        house: BuildingKind,
        kind: UnitKind,
        wants: impl Fn(&Context<'_>, &Building) -> bool,
    ) -> Done {
        let cost = price(kind);
        let mut busy = false;
        for i in 0..ctx.buildings.len() {
            let b = &ctx.buildings[i];
            if b.kind != house || !b.complete || ctx.draining.contains(&b.id) || !wants(ctx, b) {
                continue;
            }
            let id = b.id;
            if b.queue_length > 0 || b.researching.is_some() {
                busy = true; // a Town Center queued with villagers: save so the queue drains
                continue;
            }
            let key = format!("train:{id}");
            if ctx.board.cooling(&key, ctx.tick) || ctx.free_population() <= 0 {
                continue;
            }
            if crate::roster::can_recruit(ctx, kind) && ctx.try_spend(cost)? {
                let order = Order::Train {
                    building: EntityId(id),
                    kind,
                };
                ctx.order_or_stop("train", order)?;
                ctx.queued += 1;
                let wait = 10 * ctx.hz();
                ctx.board.cool(&key, ctx.tick, wait);
                // The count this order is waiting to exceed includes the
                // orders still training, so one arrival clears only the oldest.
                let pending = Self::fielded(ctx, kind) + Self::in_production(ctx, kind);
                let tick = ctx.tick;
                ctx.board
                    .producing
                    .entry(format!("{kind:?}"))
                    .or_default()
                    .push((tick, pending));
                ctx.board.bump(&format!("{}_trained", lower(kind)));
                return Ok(());
            }
            self.save(ctx, kind, cost);
            return Ok(());
        }
        if busy {
            self.save(ctx, kind, cost);
            if house == BuildingKind::TownCenter && ctx.saving_castle {
                ctx.hold_military_training = true;
            }
        }
        Ok(())
    }

    /// Reserve the manager's price and let the army spend any surplus.
    pub fn save(&self, ctx: &mut Context<'_>, kind: UnitKind, cost: [u32; 4]) {
        let cfg = ctx.config;
        let key = format!("save:{kind:?}");
        if !cfg.save_for_management
            || ctx.board.cooling(&key, ctx.tick)
            || buildorder::stage(ctx) == Stage::ImperialAdvance
        {
            return; // the Imperial Age price comes first once the prerequisites stand
        }
        let hz = ctx.hz();
        let tick = ctx.tick;
        let since = *ctx.board.save_since.entry(key.clone()).or_insert(tick);
        if tick.saturating_sub(since) > cfg.save_seconds * hz {
            ctx.board.save_since.remove(&key);
            ctx.board.cool(&key, tick, cfg.save_pause_seconds * hz);
            return;
        }
        ctx.saving_castle = true;
        ctx.board.saving_tick = tick as i64;
        ctx.board.hold_villagers_tick = tick as i64; // villagers eat the 1,000 food: pause them meanwhile
        ctx.reserve(cost);
        ctx.board.bump("saving_for_management");
    }

    /// Keep a small local workforce available for native Vassal fortification.
    pub fn defenders(&mut self, ctx: &mut Context<'_>) -> Done {
        let castles: Vec<Building> = ctx
            .buildings
            .iter()
            .filter(|b| {
                b.complete
                    && b.kind == BuildingKind::Castle
                    && garrisoned(b, UnitKind::Vassal)
                    && threatened(ctx, b.center())
            })
            .cloned()
            .collect();
        let previous: BTreeSet<u32> = ctx
            .board
            .vassal_workers
            .values()
            .flatten()
            .copied()
            .collect();
        let active: BTreeSet<u32> = castles.iter().map(|b| b.id).collect();
        ctx.board.vassal_workers.retain(|i, _| active.contains(i));
        let retained: BTreeSet<u32> = ctx
            .board
            .vassal_workers
            .values()
            .flatten()
            .copied()
            .collect();
        for worker in previous.difference(&retained) {
            ctx.reserved_workers.remove(worker);
        }
        if !ctx
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::TownCenter && b.complete)
        {
            return Ok(()); // recovery: workers stay free to rebuild the Town Center
        }
        let alive: BTreeSet<u32> = ctx.villagers.iter().map(|u| u.id).collect();
        ctx.board.vassal_workers = castles
            .iter()
            .map(|b| {
                let kept = ctx.board.vassal_workers.get(&b.id).cloned();
                let kept = kept.unwrap_or_default();
                (b.id, kept.intersection(&alive).copied().collect())
            })
            .collect();
        for castle in castles {
            let held = ctx.board.vassal_workers[&castle.id].len();
            if held >= 8 {
                continue;
            }
            let near = (castle.x - 3, castle.y - 3);
            let center = castle.center();
            let mut candidates: Vec<Unit> = ctx
                .villagers
                .iter()
                .filter(|v| {
                    !ctx.busy.contains(&v.id)
                        && !ctx.reserved_workers.contains(&v.id)
                        && !matches!(
                            v.task,
                            UnitTask::Building | UnitTask::Farming | UnitTask::Seeding
                        )
                        && dist(v.tile(), center) < 90.0
                })
                .cloned()
                .collect();
            candidates.sort_by_key(|v| dist2(v.tile(), center));
            for worker in candidates.into_iter().take(8 - held) {
                if ctx.land_route(worker.tile(), near).is_none() {
                    continue;
                }
                let order = Order::Move {
                    unit: EntityId(worker.id),
                    to: TileCoord::new(near.0, near.1),
                };
                ctx.order_or_stop("move", order)?;
                if let Some(workers) = ctx.board.vassal_workers.get_mut(&castle.id) {
                    workers.insert(worker.id);
                }
                ctx.reserved_workers.insert(worker.id);
            }
        }
        Ok(())
    }

    /// Apply ordinary UI management switches once per change.
    pub fn settings(&mut self, ctx: &mut Context<'_>) -> Done {
        let weights = shares(ctx).map(|share| (share * 100.0).round_ties_even() as u8);
        let training = !(ctx.hold_town_center || ctx.board.hold_villagers_tick == ctx.tick as i64);
        let mut seated: Vec<(u32, UnitKind, bool)> = Vec::new();
        for b in &ctx.buildings {
            for entry in &b.garrison {
                let enabled = entry.1 == UnitKind::Vassal && threatened(ctx, b.center());
                seated.push((entry.0, entry.1, enabled));
            }
        }
        for (unit, kind, enabled) in seated {
            if kind == UnitKind::Steward {
                let desired = (weights, 100, training);
                if ctx.board.manager_settings.get(&unit) != Some(&desired) {
                    let order = Order::StewardSettings {
                        unit: EntityId(unit),
                        weights,
                        cap: 100,
                        training,
                    };
                    ctx.order_or_stop("steward_settings", order)?;
                    ctx.board.manager_settings.insert(unit, desired);
                }
            } else if kind == UnitKind::Vassal {
                // A Vassal's entry keeps only the switch last sent.
                let desired = ([0; 4], 0, enabled);
                if ctx.board.manager_settings.get(&unit) != Some(&desired) {
                    let order = Order::VassalDevelopment {
                        unit: EntityId(unit),
                        enabled,
                    };
                    ctx.order_or_stop("vassal_development", order)?;
                    ctx.board.manager_settings.insert(unit, desired);
                }
            }
        }
        Ok(())
    }
}
