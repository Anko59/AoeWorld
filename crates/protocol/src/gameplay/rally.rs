use serde::{Deserialize, Serialize};

/// A production building's destination for newly trained units.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RallyTarget {
    Ground(aoe_core::WorldPosition),
    Resource(u64),
    Building(aoe_core::EntityId),
    Animal(aoe_core::EntityId),
}

/// Last accepted location remains available if the target disappears.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RallyPoint {
    pub target: RallyTarget,
    pub position: aoe_core::WorldPosition,
}
