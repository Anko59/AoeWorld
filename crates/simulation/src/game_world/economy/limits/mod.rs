//! Population-scaled world storage bounds. These are limits, not allocations.
use super::MAX_POPULATION;

pub const MAX_ECONOMY_PLAYERS: usize = 128;
/// Buildings (each wall segment is one) a seat may add to the world at the
/// baseline 200 population: about 40 houses, 100 farms, the production and
/// economy buildings, towers and several hundred wall segments.
pub const BUILDINGS_PER_PLAYER: usize = 1_024;
/// The most buildings per seat whatever the population ceiling.
pub const MAX_BUILDINGS_PER_PLAYER: usize = 8_192;
/// World-wide capacity limits, sized per seat: a world of `players` seats
/// whose players may each reach `population` never fills them in play.
/// Workers (gatherers, builders, hunters) are units, so one per unit of
/// population covers every seat; buildings get `BUILDINGS_PER_PLAYER` per seat.
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
    /// The population ceiling of one player, however many houses stand.
    pub population: u16,
}

impl WorldLimits {
    /// The limits of a match of `players` seats whose players may each reach
    /// `population`. Buildings per seat grow with a ceiling above the shipped
    /// baseline 200 (up to `MAX_BUILDINGS_PER_PLAYER`); workers are one per unit of
    /// population. Never below the limits of a 16-seat world.
    pub fn for_match(players: usize, population: u16) -> Self {
        let seats = players.clamp(16, usize::from(u16::MAX));
        let units = usize::from(population).max(usize::from(MAX_POPULATION));
        let per_seat = (BUILDINGS_PER_PLAYER * units / 200).min(MAX_BUILDINGS_PER_PLAYER);
        Self {
            players: MAX_ECONOMY_PLAYERS.max(players),
            buildings: seats * per_seat,
            gatherers: seats * units,
            builders: seats * units,
            hunters: seats * units,
            animals: seats * crate::ANIMALS_PER_PLAYER,
            // A shot lands before its shooter's next, so one per unit.
            projectiles: seats * units,
            population,
        }
    }
}

impl Default for WorldLimits {
    fn default() -> Self {
        Self::for_match(MAX_ECONOMY_PLAYERS, MAX_POPULATION)
    }
}
