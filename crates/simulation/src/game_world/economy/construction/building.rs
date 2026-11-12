//! Persistent building state, including gate admission and construction.
use super::{BuildingKind, ConstructionState, GarrisonedUnit, RallyPoint, TrainingEntry};
use aoe_core::{EntityId, PlayerId, TileRect};
use std::collections::VecDeque;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameBuilding {
    /// Age of this building, retained in player observations.
    pub age: crate::Age,
    pub id: EntityId,
    pub player: PlayerId,
    pub kind: BuildingKind,
    pub footprint: TileRect,
    pub training_queue: VecDeque<TrainingEntry>,
    pub last_hit: Option<crate::game_world::HitRecord>,
    pub state: ConstructionState,
    /// Work done and work needed; one builder adds 3 per tick, n add n + 2.
    pub work_done: u64,
    pub work_total: u64,
    pub hp: u16,
    pub max_hp: u16,
    pub los: u8,
    /// Food left on a farm; `Some(0)` is an exhausted farm.
    pub food_remaining: Option<u16>,
    pub research: Option<crate::game_world::ResearchEntry>,
    /// Authoritative gate state, also retained in fog observations.
    pub gate_open: bool,
    /// A lock blocks planning even while a unit holds the physical gate open.
    pub gate_locked: bool,
    pub rally_point: Option<RallyPoint>,
    /// Prepaid farm rebuilds held by a Mill (at most 99).
    pub farm_reseeds: u8,
    pub garrison: Vec<GarrisonedUnit>,
    pub bridge_span: Option<aoe_core::BridgeSpan>,
}
impl GameBuilding {
    /// Construction progress in thousandths, 1000 once complete.
    pub fn progress_permille(&self) -> u16 {
        if self.state == ConstructionState::Complete || self.work_total == 0 {
            return 1000;
        }
        (self.work_done.saturating_mul(1000) / self.work_total).min(1000) as u16
    }
    pub fn is_complete(&self) -> bool {
        self.state == ConstructionState::Complete
    }
}
