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
