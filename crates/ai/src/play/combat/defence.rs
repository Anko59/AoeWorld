//! The defensive half of the combat manager: predators, engagements, the
//! defence of home and the retreat of endangered villagers.
use std::collections::BTreeMap;

use aoe_sdk::{AnimalKind, AnimalTask, EntityId, Order, TileCoord};

use super::march::{Purpose, march};
use super::{CombatManager, walking};
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{clamp_tile, dist, nearest};
use crate::view::{Tile, Unit};

fn attack_order(unit: u32, target: u32) -> Order {
    Order::Attack {
        unit: EntityId(unit),
        target: EntityId(target),
    }
}

/// Sort by distance to `at`; equally near ones keep their order.
fn by_distance(units: &mut [Unit], at: Tile) {
    units.sort_by(|a, b| dist(a.tile(), at).total_cmp(&dist(b.tile(), at)));
}

impl CombatManager {
    /// Use nearby soldiers or the scout; otherwise workers fight together.
    pub fn protect_workers(&self, ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
        let cfg = ctx.config;
        for animal in ctx.world.animals() {
            let predator = animal.kind == AnimalKind::Wolf
                || (animal.kind == AnimalKind::Boar && animal.task == AnimalTask::Attacking);
            if animal.hp == 0 || !predator {
                continue;
            }
            let at = animal.tile();
            let mut workers: Vec<Unit> = ctx
                .villagers
                .iter()
                .filter(|v| v.alive && dist(v.tile(), at) <= f64::from(cfg.predator_worker_radius))
                .cloned()
                .collect();
            by_distance(&mut workers, at);
            if workers.is_empty() {
                continue;
            }
            let mut defenders: Vec<Unit> = ctx
                .army
                .iter()
                .chain(&ctx.scouts)
                .filter(|u| {
                    u.alive
                        && (!ctx.busy.contains(&u.id) || u.attack_target == Some(animal.id))
                        && self.available_target(ctx, u.id, animal.id)
                        && dist(u.tile(), at) <= f64::from(cfg.predator_defender_radius)
                })
                .cloned()
                .collect();
            by_distance(&mut defenders, at);
            defenders.truncate(cfg.predator_soldier_group as usize);
            workers.truncate(cfg.predator_worker_group as usize);
            let chosen = if defenders.is_empty() {
                workers
            } else {
                defenders
            };
            for defender in chosen {
                if defender.attack_target == Some(animal.id) {
                    ctx.busy.insert(defender.id);
                    continue;
                }
                if ctx.busy.contains(&defender.id) {
                    continue;
                }
                if self.available_target(ctx, defender.id, animal.id) {
                    ctx.order_or_stop("attack", attack_order(defender.id, animal.id))?;
                    ctx.board.builders.remove(&defender.id);
                    ctx.board.bump("predator_defences");
                }
            }
        }
        Ok(())
    }

    pub fn available_target(&self, ctx: &Context<'_>, unit: u32, target: u32) -> bool {
        !ctx.board
            .cooling(&format!("attack:{unit}:{target}"), ctx.tick)
    }

    /// Close groups fight enemies they can see without probing a route per soldier.
    pub fn engage(
        &self,
        ctx: &mut Context<'_>,
        army: &[Unit],
        enemies: &[Unit],
    ) -> Result<(), OrderBudgetExhausted> {
        if army.is_empty() || enemies.is_empty() {
            return Ok(());
        }
        let cfg = ctx.config;
        let radius = cfg.combat_engage_tiles;
        let mut cells: BTreeMap<Tile, Vec<&Unit>> = BTreeMap::new();
        for enemy in enemies.iter().filter(|e| e.alive) {
            let cell = (enemy.x.div_euclid(radius), enemy.y.div_euclid(radius));
            let group = cells.entry(cell).or_default();
            if group.len() < cfg.combat_targets_per_cell {
                group.push(enemy);
            }
        }
        let mut origins: BTreeMap<Tile, Tile> = BTreeMap::new();
        // Groups in the order their first pick was made.
        let mut picks: Vec<Vec<(&Unit, &Unit)>> = Vec::new();
        let mut index: BTreeMap<Tile, usize> = BTreeMap::new();
        let limit = ctx.land_query_limit;
        ctx.land_query_limit = limit.min(ctx.land_queries() + cfg.combat_engage_route_queries);
        for unit in army {
            if unit.attack_target.is_some() || ctx.busy.contains(&unit.id) {
                continue;
            }
            let cell = (unit.x.div_euclid(radius), unit.y.div_euclid(radius));
            let origin = *origins.entry(cell).or_insert(unit.tile());
            let mut targets: Vec<&Unit> = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let group = cells.get(&(cell.0 + dx, cell.1 + dy));
                    for enemy in group.into_iter().flatten() {
                        if dist(unit.tile(), enemy.tile()) <= f64::from(radius)
                            && self.available_target(ctx, unit.id, enemy.id)
                        {
                            targets.push(enemy);
                        }
                    }
                }
            }
            // A soldier already on the march is under attack-move: the
            // server judges the attack, and only a refusal stops it.
            let on_march = walking(unit);
            targets.sort_by(|a, b| {
                dist(unit.tile(), a.tile())
                    .total_cmp(&dist(unit.tile(), b.tile()))
                    .then(a.id.cmp(&b.id))
            });
            for target in targets {
                if on_march || ctx.land_route(origin, target.tile()).is_some() {
                    let slot = *index.entry(cell).or_insert_with(|| {
                        picks.push(Vec::new());
                        picks.len() - 1
                    });
                    picks[slot].push((unit, target));
                    break;
                }
            }
        }
        ctx.land_query_limit = limit;
        for group in &picks {
            if group.len() >= cfg.combat_group_min {
                // A dense contact is one attack-move order for the whole group.
                let goal = group[0].1.tile();
                for members in group.chunks(cfg.march_group_members) {
                    if ctx.orders >= ctx.order_limit {
                        return Ok(());
                    }
                    let order = Order::FormationMove {
                        units: members.iter().map(|(u, _)| EntityId(u.id)).collect(),
                        formation: cfg.army_formation,
                        to: TileCoord::new(goal.0, goal.1),
                        queued: false,
                        attack_move: true,
                    };
                    ctx.order_or_stop("formation_move", order)?;
                }
                continue;
            }
            for (unit, target) in group {
                if ctx.orders >= ctx.order_limit {
                    return Ok(());
                }
                ctx.order_or_stop("attack", attack_order(unit.id, target.id))?;
            }
        }
        Ok(())
    }

    /// Engage enemies near home. True when the army is busy defending.
    pub fn defend(
        &self,
        ctx: &mut Context<'_>,
        army: &[Unit],
    ) -> Result<bool, OrderBudgetExhausted> {
        let Some(home) = ctx.home else {
            return Ok(false);
        };
        let cfg = ctx.config;
        let mut candidates: Vec<Unit> = ctx
            .enemies
            .iter()
            .filter(|e| dist(e.tile(), home) <= f64::from(cfg.defend_radius))
            .cloned()
            .collect();
        candidates.sort_by(|a, b| {
            dist(a.tile(), home)
                .total_cmp(&dist(b.tile(), home))
                .then(a.id.cmp(&b.id))
        });
        // Bound attack-target queries, leaving work for the soldiers' routes.
        // Visible raiders outside this shortlist still count toward danger.
        let shortlist = ((cfg.land_queries_per_decision / 4) as usize).max(1);
        let mut near: Vec<Unit> = Vec::new();
        for enemy in candidates.iter().take(shortlist) {
            if ctx.land_route(home, enemy.tile()).is_some() {
                near.push(enemy.clone());
            }
        }
        if near.is_empty() {
            return Ok(false);
        }
        // A lone raider (an enemy scout) is not worth abandoning the economy for.
        if candidates.len() >= 3 && army.len() * 2 < candidates.len() {
            self.pull_back(ctx, &candidates, None, 20)?;
        }
        if army.is_empty() {
            return Ok(true);
        }
        let mut sent = 0;
        for unit in army {
            if sent >= 30 {
                break;
            }
            if !ctx.busy.contains(&unit.id) && (unit.idle || unit.attack_target.is_none()) {
                let open: Vec<Unit> = near
                    .iter()
                    .filter(|e| self.available_target(ctx, unit.id, e.id))
                    .cloned()
                    .collect();
                let Some(target) = nearest(&open, unit.tile(), Unit::tile) else {
                    continue;
                };
                if ctx.land_route(unit.tile(), target.tile()).is_none() {
                    continue;
                }
                if dist(unit.tile(), target.tile()) > f64::from(cfg.march_hop_tiles) {
                    let alone = std::slice::from_ref(unit);
                    march(ctx, alone, unit.tile(), target.tile(), Purpose::Attack)?;
                } else {
                    ctx.order_or_stop("attack", attack_order(unit.id, target.id))?;
                }
                // Reserve a paused defender too: rally must not override it.
                ctx.busy.insert(unit.id);
                sent += 1;
            }
        }
        ctx.board.bump("defences");
        Ok(sent > 0)
    }

    /// Order endangered villagers back to a Town Center when outnumbered.
    pub fn pull_back(
        &self,
        ctx: &mut Context<'_>,
        near: &[Unit],
        shelter: Option<Tile>,
        limit: usize,
    ) -> Result<(), OrderBudgetExhausted> {
        let Some(shelter) = shelter.or(ctx.home) else {
            return Ok(());
        };
        let target = clamp_tile((shelter.0 + 4, shelter.1 + 4), ctx.world.map_size(), 3);
        let mut pulled = 0;
        let villagers = ctx.villagers.clone();
        for villager in &villagers {
            if pulled >= limit || ctx.orders >= ctx.order_limit {
                break;
            }
            if !ctx.busy.contains(&villager.id)
                && dist(villager.tile(), shelter) > 3.0
                && near.iter().any(|e| dist(e.tile(), villager.tile()) <= 7.0)
            {
                let order = Order::Move {
                    unit: EntityId(villager.id),
                    to: TileCoord::new(target.0, target.1),
                };
                ctx.order_or_stop("move", order)?;
                ctx.board.builders.remove(&villager.id);
                pulled += 1;
            }
        }
        if pulled > 0 {
            ctx.board.bump_by("villagers_pulled_back", pulled as u64);
        }
        Ok(())
    }
}
