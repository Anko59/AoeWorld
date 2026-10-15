//! Current village wire state; independent of simulation storage.
use aoe_core::{EntityId, PlayerId, TileRect};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::fmt;

pub const MAX_ECONOMY_PLAYERS: usize = 16;
pub const MAX_ECONOMY_BUILDINGS: usize = 16;
pub const MAX_TRAINING_QUEUE: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UnitKind {
    Villager,
    Scout,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BuildingKind {
    TownCenter,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UnitTask {
    Idle,
    Moving,
    Gathering,
    Carrying,
    Returning,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResourceKind {
    Food,
    Wood,
    Gold,
    Stone,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CarriedResource {
    pub kind: ResourceKind,
    pub amount: u16,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Stockpile {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameplayPlayerState {
    pub player: PlayerId,
    pub stockpile: Stockpile,
    pub population_used: u16,
    pub population_cap: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrainingState {
    pub kind: UnitKind,
    pub progress_ticks: u64,
    pub total_ticks: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameplayBuildingState {
    pub id: EntityId,
    pub player: PlayerId,
    pub kind: BuildingKind,
    pub footprint: TileRect,
    #[serde(deserialize_with = "bounded_training")]
    pub training_queue: Vec<TrainingState>,
}

pub(super) fn valid_players(players: &[GameplayPlayerState]) -> bool {
    players.len() <= MAX_ECONOMY_PLAYERS
        && players.iter().all(|player| {
            player.population_used <= player.population_cap
                && player.population_cap <= (MAX_ECONOMY_BUILDINGS * 5) as u16
        })
        && players
            .windows(2)
            .all(|pair| pair[0].player < pair[1].player)
}
pub(super) fn valid_buildings(buildings: &[GameplayBuildingState]) -> bool {
    buildings.len() <= MAX_ECONOMY_BUILDINGS
        && buildings.iter().all(|building| {
            building.footprint.min.x >= 0
                && building.footprint.min.y >= 0
                && building.footprint.width() == 4
                && building.footprint.height() == 4
                && building.training_queue.len() <= MAX_TRAINING_QUEUE
                && building.training_queue.iter().all(|entry| {
                    entry.kind == UnitKind::Villager
                        && entry.total_ticks > 0
                        && entry.progress_ticks <= entry.total_ticks
                })
        })
        && buildings.windows(2).all(|pair| pair[0].id < pair[1].id)
}
pub(super) fn valid_units(units: &[super::UnitState]) -> bool {
    units.iter().all(|unit| {
        unit.facing < 8
            && unit.carried.is_none_or(|load| {
                unit.kind == UnitKind::Villager && load.amount > 0 && load.amount <= 10
            })
    })
}

fn bounded<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Items<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Items<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {N} economy entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(serde::de::Error::custom("economy collection exceeds limit"));
            }
            let mut items = Vec::new();
            while items.len() < N {
                match sequence.next_element()? {
                    Some(item) => items.push(item),
                    None => return Ok(items),
                }
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("economy collection exceeds limit"));
            }
            Ok(items)
        }
    }
    deserializer.deserialize_seq(Items::<T, LIMIT>(std::marker::PhantomData))
}
pub(super) fn bounded_players<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GameplayPlayerState>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_PLAYERS>(d)
}
pub(super) fn bounded_buildings<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GameplayBuildingState>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_BUILDINGS>(d)
}
pub(super) fn bounded_building_removals<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<EntityId>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_BUILDINGS>(d)
}
fn bounded_training<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<TrainingState>, D::Error> {
    bounded::<D, _, MAX_TRAINING_QUEUE>(d)
}
