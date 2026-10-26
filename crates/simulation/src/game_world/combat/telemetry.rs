//! Observational counters: no clock, entity history, or idle-tick work.
//! Excluded from the gameplay hash: these never influence future simulation.
use crate::GameWorld;
use aoe_core::PlayerId;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTelemetry {
    pub melee_hits: u64,
    pub ranged_hits: u64,
    pub deaths: u64,
}

#[derive(Debug, Default)]
pub(super) struct Telemetry {
    events: VecDeque<(u64, CombatTelemetry)>,
    totals: BTreeMap<PlayerId, (u64, u64)>,
}

impl GameWorld {
    /// Resolved hits and deaths of player units/buildings over the last sixty
    /// simulation seconds. Wildlife targets are excluded. Read cost is at most one
    /// small counter per tick in that minute, independent of entity count.
    pub fn combat_telemetry(&self) -> CombatTelemetry {
        let mut total = CombatTelemetry::default();
        let span = u64::from(self.config.tick_hz) * 60;
        for (tick, counts) in &self.combat.telemetry.events {
            if self.tick.0.saturating_sub(*tick) < span {
                total.melee_hits += counts.melee_hits;
                total.ranged_hits += counts.ranged_hits;
                total.deaths += counts.deaths;
            }
        }
        total
    }

    /// Match lifetime kills/losses; they survive corpse and building removal.
    pub fn player_combat_totals(&self, player: PlayerId) -> (u64, u64) {
        self.combat
            .telemetry
            .totals
            .get(&player)
            .copied()
            .unwrap_or_default()
    }

    pub(in crate::game_world) fn record_combat_hit(
        &mut self,
        attacker: Option<PlayerId>,
        victim: PlayerId,
        ranged: bool,
        killed: bool,
    ) {
        let now = self.tick.0;
        let span = u64::from(self.config.tick_hz) * 60;
        let telemetry = &mut self.combat.telemetry;
        while telemetry
            .events
            .front()
            .is_some_and(|(tick, _)| now.saturating_sub(*tick) >= span)
        {
            telemetry.events.pop_front();
        }
        if telemetry.events.back().is_none_or(|(tick, _)| *tick != now) {
            telemetry
                .events
                .push_back((now, CombatTelemetry::default()));
        }
        if let Some((_, counts)) = telemetry.events.back_mut() {
            counts.melee_hits += u64::from(!ranged);
            counts.ranged_hits += u64::from(ranged);
            counts.deaths += u64::from(killed);
        }
        if killed {
            telemetry.totals.entry(victim).or_default().1 += 1;
            if let Some(attacker) = attacker.filter(|player| *player != victim) {
                telemetry.totals.entry(attacker).or_default().0 += 1;
            }
        }
    }
}
