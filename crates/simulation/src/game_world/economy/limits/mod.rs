//! World storage budgets. None of these is a player's population ceiling.

pub const MAX_ECONOMY_PLAYERS: usize = 128;
/// Building storage per seat, including individual wall segments.
pub const BUILDINGS_PER_PLAYER: usize = 8_192;
/// Storage for each worker job and projectile pool per seat. This bounds
/// auxiliary state, does not reserve memory, and never blocks unit training.
const JOBS_PER_PLAYER: usize = 65_536;
/// World-wide storage limits sized by seats, independent of housing and
/// population. A capacity refusal reports a world-storage limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldLimits {
    pub players: usize,
    pub buildings: usize,
    pub gatherers: usize,
    pub builders: usize,
    pub hunters: usize,
    pub animals: usize,
    /// Arrows and javelins in flight; a shot past it is not launched.
    pub projectiles: usize,
}

impl WorldLimits {
    /// Logical storage budgets for `players` seats. Never below the budgets
    /// of a 16-seat world; entries are allocated only when they are used.
    pub fn for_match(players: usize) -> Self {
        let seats = players.clamp(16, usize::from(u16::MAX));
        Self {
            players: MAX_ECONOMY_PLAYERS.max(players),
            buildings: seats * BUILDINGS_PER_PLAYER,
            gatherers: seats * JOBS_PER_PLAYER,
            builders: seats * JOBS_PER_PLAYER,
            hunters: seats * JOBS_PER_PLAYER,
            animals: seats * crate::ANIMALS_PER_PLAYER,
            projectiles: seats * JOBS_PER_PLAYER,
        }
    }
}

impl Default for WorldLimits {
    fn default() -> Self {
        Self::for_match(MAX_ECONOMY_PLAYERS)
    }
}
