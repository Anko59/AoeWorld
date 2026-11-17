//! Experiment (#865 Q6, never merged as is): how often could an unseen
//! squad's members simply be placed on their slots? A member is placeable
//! when its slot target is open ground for its body; otherwise a body model
//! would fall back to stepping it individually.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static PLACEABLE: AtomicU64 = AtomicU64::new(0);
static FALLBACK: AtomicU64 = AtomicU64::new(0);
const HUMAN: aoe_core::PlayerId = aoe_core::PlayerId(0);

impl GameWorld {
    /// Counts this tick's unseen, non-fighting members by whether their slot
    /// target is statically clear.
    pub(crate) fn census_bodies(&self, movers: &[EntityId]) {
        let (mut placeable, mut fallback) = (0, 0);
        for &id in movers {
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            let unit = self.units[index].state;
            if self.unit_visible_to(HUMAN, id)
                || self.combat.attackers.contains_key(&id)
                || unit.attack_target.is_some()
            {
                continue;
            }
            let Some(target) = self.formation_motion(index).map(|m| m.target) else {
                continue;
            };
            let open = self.config.valid_ground_position(target)
                && !self.economy.blocked_tiles.contains(&target.tile_floor())
                && self
                    .unit_tile_passable(unit.kind, target.tile_floor(), &|| false)
                    .unwrap_or(false)
                && self.body_position_clear(unit.kind, target);
            if open {
                placeable += 1;
            } else {
                fallback += 1;
            }
        }
        PLACEABLE.fetch_add(placeable, Ordering::Relaxed);
        FALLBACK.fetch_add(fallback, Ordering::Relaxed);
    }

    /// Member-ticks counted so far: (placeable, fallback).
    pub fn body_census() -> (u64, u64) {
        (
            PLACEABLE.load(Ordering::Relaxed),
            FALLBACK.load(Ordering::Relaxed),
        )
    }
}
