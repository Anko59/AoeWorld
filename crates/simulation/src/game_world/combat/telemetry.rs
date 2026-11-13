//! Observational counters: no clock, entity history, or idle-tick work.
//! Excluded from the gameplay hash: these never influence future simulation.
use crate::GameWorld;
use aoe_core::PlayerId;
use std::collections::{BTreeMap, VecDeque};

type ProjectionTotals = (
    u64,
    u64,
    Option<(
        aoe_core::EntityId,
        aoe_core::EntityId,
        aoe_core::TileCoord,
        u64,
    )>,
);

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
    projection: BTreeMap<PlayerId, ProjectionTotals>,
}

impl GameWorld {
    /// Enemy buildings destroyed, all unit losses, and the latest building kill
    /// (target, attacker unit, tile, tick). Observation only; no entity scans.
    pub fn player_projection_totals(&self, player: PlayerId) -> ProjectionTotals {
        self.combat
            .telemetry
            .projection
            .get(&player)
            .copied()
            .unwrap_or_default()
    }

    pub(in crate::game_world) fn record_unit_loss(&mut self, player: PlayerId) {
        self.combat
            .telemetry
            .projection
            .entry(player)
            .or_default()
            .1 += 1;
    }

    pub(in crate::game_world) fn record_building_kill(
        &mut self,
        player: PlayerId,
        victim: PlayerId,
        target: aoe_core::EntityId,
        attacker: aoe_core::EntityId,
        tile: aoe_core::TileCoord,
    ) {
        if player == victim {
            return;
        }
        let totals = self.combat.telemetry.projection.entry(player).or_default();
        totals.0 += 1;
        totals.2 = Some((target, attacker, tile, self.tick.0));
    }
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
