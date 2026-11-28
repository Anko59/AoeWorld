//! Posture shared by the authoritative simulation and its clients.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum CombatStance {
    #[default]
    Aggressive,
    Defensive,
    StandGround,
    NoAttack,
}

/// One seat's relationship to another. An alliance always binds both seats.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiplomaticStance {
    #[default]
    Enemy,
    Neutral,
    Ally,
}
