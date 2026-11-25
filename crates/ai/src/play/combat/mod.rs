//! Fighting: defend the base, mass an army, attack the nearest enemy when
//! strong, and pull back when the attack is failing
//! (`ai/aoe_ai/play/combat/__init__.py`). The manager keeps no state of its
//! own: like Python's, it lives on the blackboard.
mod defence;
pub mod march;
mod offence;
pub mod raid;

use std::collections::{BTreeMap, BTreeSet};

use aoe_sdk::{EntityId, Event, Order, UnitTask};

use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{centroid, dist, near_any};
use crate::view::{Tile, Unit};

/// A soldier on the march: reported moving, or with the moving task.
fn walking(unit: &Unit) -> bool {
    unit.moving || unit.task == UnitTask::Moving
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CombatManager {
    /// Python's manager keeps nothing: its state is on the blackboard.
    stateless: std::marker::PhantomData<()>,
}

impl CombatManager {
    pub fn step(&self, ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
        if ctx.home.is_none() {
            return Ok(());
        }
        raid::observe(ctx);
        raid::watch_sites(ctx);
        self.protect_workers(ctx)?;
        self.answer_fire(ctx)?;
        self.recover_marches(ctx)?;
        let army: Vec<Unit> = ctx
            .army
            .iter()
            .filter(|u| u.alive && !ctx.busy.contains(&u.id))
            .cloned()
            .collect();
        if army.is_empty() && ctx.army.iter().any(|u| u.alive) {
            return Ok(());
        }
        if raid::respond(ctx, &army, self)? || self.defend(ctx, &army)? {
            return Ok(());
        }
        let enemies = ctx.enemies.clone();
        self.engage(ctx, &army, &enemies)?;
        let mut attack_size = ctx.config.attack_army_size;
        let mem = &*ctx.board;
        if mem.counter_owner.is_some() && (ctx.tick as i64) < mem.counter_until {
            attack_size = attack_size.min(ctx.config.counter_min_soldiers);
        }
        if ctx.board.attacking {
            let field = self.field_army(ctx, &army);
            self.attack(ctx, &field)?;
        } else if army.len() >= attack_size as usize {
            let formed = self.assembled(ctx, &army);
            if formed.len() >= attack_size as usize {
                let mem = &mut *ctx.board;
                mem.attacking = true;
                mem.attack_launch_size = formed.len() as u32;
                mem.attack_members = formed.iter().map(|u| u.id).collect();
                mem.bump("attacks_launched");
                self.attack(ctx, &formed)?;
            }
        }
        Ok(())
    }

    /// Find the strongest local muster, including troops across cell edges.
    pub fn assembled(&self, ctx: &Context<'_>, army: &[Unit]) -> Vec<Unit> {
        let size = ctx.config.army_muster_tiles;
        let home = ctx.home.unwrap_or_default();
        let mut cells: BTreeMap<Tile, Vec<&Unit>> = BTreeMap::new();
        for unit in army {
            cells
                .entry((unit.x.div_euclid(size), unit.y.div_euclid(size)))
                .or_default()
                .push(unit);
        }
        let adjacent = |cell: Tile| -> Vec<&Unit> {
            let mut found = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(group) = cells.get(&(cell.0 + dx, cell.1 + dy)) {
                        found.extend(group);
                    }
                }
            }
            found
        };
        // (-members, distance of the cell's middle to home, cell): lowest wins.
        let score = |cell: Tile| {
            let middle = (cell.0 * size + size / 2, cell.1 * size + size / 2);
            (-(adjacent(cell).len() as i64), dist(middle, home))
        };
        let best = cells.keys().copied().min_by(|a, b| {
            let (sa, sb) = (score(*a), score(*b));
            sa.0.cmp(&sb.0).then(sa.1.total_cmp(&sb.1)).then(a.cmp(b))
        });
        best.map_or_else(Vec::new, |cell| {
            adjacent(cell).into_iter().cloned().collect()
        })
    }

    pub fn field_army(&self, ctx: &mut Context<'_>, available: &[Unit]) -> Vec<Unit> {
        if ctx.board.attack_members.is_empty() {
            return available.to_vec();
        }
        let members = &mut ctx.board.attack_members;
        let mut field: Vec<Unit> = ctx
            .army
            .iter()
            .filter(|u| u.alive && members.contains(&u.id))
            .cloned()
            .collect();
        let kept: BTreeSet<u32> = field.iter().map(|u| u.id).collect();
        members.retain(|id| kept.contains(id));
        let tiles: Vec<Tile> = field.iter().map(Unit::tile).collect();
        if let Some(middle) = centroid(&tiles) {
            let reach = f64::from(ctx.config.army_muster_tiles);
            let reinforcements: Vec<Unit> = available
                .iter()
                .filter(|u| !members.contains(&u.id) && dist(u.tile(), middle) <= reach)
                .cloned()
                .collect();
            members.extend(reinforcements.iter().map(|u| u.id));
            field.extend(reinforcements);
        }
        field
    }

    /// A soldier hit on the march fights back, or halts when it cannot see
    /// its attacker, instead of walking on while it is killed.
    pub fn answer_fire(&self, ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
        let events = ctx.events;
        let hits: Vec<(u32, Option<u32>)> = events
            .iter()
            .filter_map(|event| match event {
                Event::UnderAttack {
                    target,
                    attacker,
                    building: false,
                    ..
                } => Some((target.0, attacker.map(|a| a.0))),
                _ => None,
            })
            .collect();
        if hits.is_empty() {
            return Ok(());
        }
        let soldiers: BTreeMap<u32, Unit> = ctx
            .army
            .iter()
            .filter(|u| u.alive)
            .map(|u| (u.id, u.clone()))
            .collect();
        let enemies: BTreeMap<u32, Unit> = ctx.enemies.iter().map(|e| (e.id, e.clone())).collect();
        for (id, attacker) in hits {
            let Some(unit) = soldiers.get(&id) else {
                continue;
            };
            if ctx.busy.contains(&unit.id) || unit.attack_target.is_some() {
                continue;
            }
            let attacker = attacker.and_then(|a| enemies.get(&a));
            if let Some(attacker) =
                attacker.filter(|a| a.alive && self.available_target(ctx, unit.id, a.id))
            {
                let order = Order::Attack {
                    unit: EntityId(unit.id),
                    target: EntityId(attacker.id),
                };
                ctx.order_or_stop("attack", order)?;
            } else if walking(unit) {
                ctx.order_or_stop(
                    "stop",
                    Order::Stop {
                        unit: EntityId(unit.id),
                    },
                )?;
                ctx.board.marched.insert(unit.id, ctx.tick);
            } else {
                continue;
            }
            ctx.board.march_progress.remove(&unit.id);
            ctx.board.bump("fire_answered");
        }
        Ok(())
    }

    /// Release accepted routes that stopped making observable progress.
    ///
    /// Formation members can remain MOVING while waiting on one another.
    /// Stop only stationary soldiers after the normal stall interval, letting
    /// the next decision plan a fresh route without interrupting a working one.
    pub fn recover_marches(&self, ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
        let live: BTreeSet<u32> = ctx.army.iter().filter(|u| u.alive).map(|u| u.id).collect();
        ctx.board.march_progress.retain(|id, _| live.contains(id));
        let stall = ctx.config.march_stall_seconds * ctx.hz();
        // Soldiers in contact with enemies are held up by the fight, not stuck.
        let tiles: Vec<Tile> = ctx
            .enemies
            .iter()
            .filter(|e| e.alive)
            .map(Unit::tile)
            .collect();
        let contact = near_any(&tiles, f64::from(ctx.config.combat_engage_tiles));
        let army = ctx.army.clone();
        for unit in &army {
            if !walking(unit)
                || unit.attack_target.is_some()
                || ctx.busy.contains(&unit.id)
                || (!ctx.enemies.is_empty() && contact(unit.tile()))
            {
                ctx.board.march_progress.remove(&unit.id);
                continue;
            }
            let (mut tile, mut since) = ctx
                .board
                .march_progress
                .get(&unit.id)
                .copied()
                .unwrap_or((unit.tile(), ctx.tick));
            if dist(tile, unit.tile()) >= 2.0 {
                (tile, since) = (unit.tile(), ctx.tick);
            }
            ctx.board.march_progress.insert(unit.id, (tile, since));
            if ctx.tick.saturating_sub(since) >= stall {
                ctx.order_or_stop(
                    "stop",
                    Order::Stop {
                        unit: EntityId(unit.id),
                    },
                )?;
                let mem = &mut *ctx.board;
                mem.march_progress.remove(&unit.id);
                mem.marched.remove(&unit.id);
                mem.cool(&format!("march-alone:{}", unit.id), ctx.tick, 2 * stall);
                mem.march_goal = None;
                mem.bump("marches_recovered");
            }
        }
        Ok(())
    }
}
