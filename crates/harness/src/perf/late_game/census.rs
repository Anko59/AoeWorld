//! Activity requirements shared by timing and deterministic kernel setup.
use aoe_simulation::{GameWorld, UnitKind, UnitTask};
use serde::Serialize;
use std::error::Error;

#[derive(Clone, Default, Debug, Serialize)]
pub(in crate::perf) struct Census {
    pub(in crate::perf) players: usize,
    pub(in crate::perf) units: usize,
    pub(in crate::perf) gathering: usize,
    pub(in crate::perf) returning: usize,
    pub(in crate::perf) moving: usize,
    pub(in crate::perf) idle_army: usize,
    pub(in crate::perf) marching_army: usize,
    pub(in crate::perf) queues: usize,
    pub(in crate::perf) queued: usize,
    pub(in crate::perf) trained_units: usize,
    pub(in crate::perf) research: usize,
    pub(in crate::perf) wildlife: usize,
    pub(in crate::perf) deposited: u32,
    pub(in crate::perf) displaced: usize,
    pub(in crate::perf) exploring: usize,
    pub(in crate::perf) formation_members: usize,
    pub(in crate::perf) garrisoned: usize,
    pub(in crate::perf) monks: usize,
    pub(in crate::perf) naval: usize,
    pub(in crate::perf) mature_queues: usize,
}
impl Census {
    pub(in crate::perf) fn take(
        world: &GameWorld,
        initial_wood: u32,
        initial_bound: usize,
    ) -> Self {
        Self {
            displaced: world
                .units()
                .filter(|u| u.moving && u.position != u.previous_position)
                .count(),
            exploring: world.units().filter(|u| u.auto_scout).count(),
            formation_members: world
                .units()
                .filter(|u| world.formation(u.id).is_some())
                .count(),
            garrisoned: world.buildings().map(|b| b.garrison.len()).sum(),
            monks: world.units().filter(|u| u.kind == UnitKind::Monk).count(),
            naval: world.units().filter(|u| u.kind.is_naval()).count(),
            mature_queues: world
                .buildings()
                .filter(|b| {
                    b.training_queue
                        .front()
                        .is_some_and(|e| e.progress_ticks == e.total_ticks)
                })
                .count(),
            players: world.players().count(),
            units: world.unit_count(),
            gathering: world
                .units()
                .filter(|u| u.task == UnitTask::Gathering)
                .count(),
            returning: world
                .units()
                .filter(|u| u.task == UnitTask::Returning)
                .count(),
            moving: world.units().filter(|u| u.moving).count(),
            marching_army: world
                .units()
                .filter(|u| u.kind != UnitKind::Villager && u.moving)
                .count(),
            idle_army: world
                .units()
                .filter(|u| u.kind != UnitKind::Villager && u.task == UnitTask::Idle)
                .count(),
            queues: world
                .buildings()
                .filter(|b| !b.training_queue.is_empty())
                .count(),
            queued: world.buildings().map(|b| b.training_queue.len()).sum(),
            trained_units: world
                .units()
                .filter(|u| u.id.0 as usize >= initial_bound)
                .count(),
            research: world.buildings().filter(|b| b.research.is_some()).count(),
            wildlife: world.animals().count(),
            deposited: world
                .players()
                .map(|p| p.stockpile.wood)
                .sum::<u32>()
                .saturating_sub(initial_wood),
        }
    }
    pub(in crate::perf) fn validate_after(&self, before: &Self) -> Result<(), Box<dyn Error>> {
        self.validate()?;
        if self.deposited <= before.deposited
            || self.trained_units <= before.trained_units
            || self.research >= before.research
        {
            return Err(format!("late-game measured interval stopped economic work: before {before:?}, after {self:?}").into());
        }
        Ok(())
    }
    /// Completed entries leave a vacancy until next tick's ordinary refill.
    pub(in crate::perf) fn validate_activity(&self) -> Result<(), Box<dyn Error>> {
        self.validate_mix(false)
    }
    pub(in crate::perf) fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.validate_mix(true)
    }
    fn validate_mix(&self, full_queues: bool) -> Result<(), Box<dyn Error>> {
        if self.players != 100
            || !(13_000..=15_000).contains(&self.units)
            || self.gathering < 1_000
            || self.returning < 100
            || self.moving < 3_000
            || self.displaced < 2_000
            || self.marching_army < 2_000
            || self.exploring != 100
            || self.formation_members != 1_200
            || self.garrisoned != 100
            || self.monks != 100
            || self.naval != 200
            || self.mature_queues < 15
            || self.queues != 200
            || (full_queues && self.queued != 1_000)
            || self.queued < 4 * self.queues
            || self.queued > 5 * self.queues
            || self.wildlife < 1_000
            || self.deposited == 0
        {
            return Err(format!("late-game task mix collapsed: {self:?}").into());
        }
        Ok(())
    }
}
