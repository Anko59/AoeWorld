//! Two armies meet: how many units that have an enemy within reach fight it?
//! The probe is shared with the General scenario in the economy tests.
use super::*;
use crate::UnitKind;
use std::collections::BTreeMap;

mod battle;

/// Reach, in tiles beyond a unit's attack range, that counts as "should fight".
const NEAR_TILES: i64 = 2;

#[derive(Default)]
pub(crate) struct Probe {
    /// Consecutive ticks a unit has had an enemy in reach without a target.
    streak: BTreeMap<EntityId, u32>,
    longest: BTreeMap<EntityId, u32>,
    last_hit_tick: BTreeMap<EntityId, u64>,
    pub in_reach: u64,
    pub with_target: u64,
    pub idle_in_reach: u64,
    pub striking_recently: u64,
    pub ticks: u64,
    /// Per 100 ticks: unit-ticks in reach, with a target, moving without one.
    pub timeline: Vec<[u64; 4]>,
}

impl Probe {
    /// Samples every unit of `ids` against every unit of the other players.
    pub(crate) fn sample(&mut self, w: &GameWorld, ids: &[EntityId]) {
        self.ticks += 1;
        if self.timeline.len() < (self.ticks as usize).div_ceil(100) {
            self.timeline.push([0; 4]);
        }
        let bucket = self.timeline.len() - 1;
        let all: Vec<_> = w.units().filter(|u| u.hp > 0).collect();
        let hz = u64::from(w.config().tick_hz);
        for &id in ids {
            let Some(u) = w.unit(id).filter(|u| u.hp > 0) else {
                continue;
            };
            let Some(stats) = w.unit_stats(id) else {
                continue;
            };
            let next = w.units[w.lookup[&id]].next_hit_tick;
            if self.last_hit_tick.get(&id).is_some_and(|&n| n != next) {
                self.last_hit_tick.insert(id, w.tick.0);
            }
            self.last_hit_tick.entry(id).or_insert(next);
            let reach = (crate::game_world::navy::attack_range(&stats)
                + NEAR_TILES * i64::from(aoe_core::FIXED_SUBUNITS_PER_TILE))
            .pow(2);
            let near = all.iter().any(|e| {
                e.player != u.player && {
                    let (dx, dy) = (
                        i64::from(e.position.x - u.position.x),
                        i64::from(e.position.y - u.position.y),
                    );
                    dx * dx + dy * dy <= reach
                }
            });
            if !near {
                self.streak.remove(&id);
                continue;
            }
            self.in_reach += 1;
            self.timeline[bucket][0] += 1;
            if u.attack_target.is_some() {
                self.with_target += 1;
                self.timeline[bucket][1] += 1;
                self.streak.remove(&id);
            } else {
                if matches!(u.task, UnitTask::Idle) {
                    self.idle_in_reach += 1;
                }
                if u.moving || u.planning {
                    self.timeline[bucket][2] += 1;
                } else {
                    self.timeline[bucket][3] += 1;
                }
                let s = self.streak.entry(id).or_default();
                *s += 1;
                let l = self.longest.entry(id).or_default();
                *l = (*l).max(*s);
            }
            if self
                .last_hit_tick
                .get(&id)
                .is_some_and(|&t| w.tick.0 <= t + hz * 2)
                && self.last_hit_tick[&id] != 0
            {
                self.striking_recently += 1;
            }
        }
    }

    /// Units whose longest target-less stretch within reach exceeded `seconds`.
    pub(crate) fn stuck_units(&self, seconds: u64, hz: u64) -> usize {
        self.longest
            .values()
            .filter(|&&n| u64::from(n) > seconds * hz)
            .count()
    }

    pub(crate) fn report(&self, name: &str, hz: u64, army: usize) -> String {
        let pct = |a: u64, b: u64| {
            if b == 0 {
                0.0
            } else {
                a as f64 * 100.0 / b as f64
            }
        };
        let line = format!(
            "{name}: timeline[in_reach,target,moving_no_target,still_no_target]/100t={:?}\n{name}: ticks={} unit_ticks_in_reach={} with_target={:.1}% idle={:.1}% struck_2s={:.1}% stuck>3s={}/{} stuck>6s={}\n",
            self.timeline,
            self.ticks,
            self.in_reach,
            pct(self.with_target, self.in_reach),
            pct(self.idle_in_reach, self.in_reach),
            pct(self.striking_recently, self.in_reach),
            self.stuck_units(3, hz),
            army,
            self.stuck_units(6, hz),
        );
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/engage");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.txt")), &line).unwrap();
        line
    }
}

pub(crate) const MIX: [UnitKind; 8] = [
    UnitKind::Militia,
    UnitKind::Spearman,
    UnitKind::Archer,
    UnitKind::Knight,
    UnitKind::Crossbowman,
    UnitKind::ManAtArms,
    UnitKind::Pikeman,
    UnitKind::Skirmisher,
];

/// A block of `count` mixed soldiers, `width` wide, from `(x, y)`.
pub(crate) fn army(
    w: &mut GameWorld,
    player: u8,
    count: i32,
    width: i32,
    x: i32,
    y: i32,
) -> Vec<EntityId> {
    (0..count)
        .map(|n| {
            w.spawn_kind(
                PlayerId(player.into()),
                MIX[n as usize % MIX.len()],
                at(x + n % width, y + n / width),
            )
            .unwrap()
        })
        .collect()
}
