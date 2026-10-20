//! Bounded wildlife frames; a frame replaces the region's entire animal set.
use super::{ClientMessage, Error, ServerMessage, economy};
use aoe_core::{EntityId, PlayerId, WorldPosition};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;

pub const MAX_ANIMALS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnimalKind {
    Sheep,
    Deer,
    Boar,
    Wolf,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnimalTask {
    Idle,
    Fleeing,
    Attacking,
    Dead,
    Carcass,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnimalState {
    pub id: EntityId,
    pub kind: AnimalKind,
    pub owner: Option<PlayerId>,
    pub hp: u16,
    pub position: WorldPosition,
    pub task: AnimalTask,
    pub food_remaining: u16,
    pub target: Option<EntityId>,
}

fn valid_animal(a: &AnimalState) -> bool {
    let (hp, food) = match a.kind {
        AnimalKind::Sheep => (7, 100),
        AnimalKind::Deer => (5, 140),
        AnimalKind::Boar => (75, 340),
        AnimalKind::Wolf => (25, 0),
    };
    a.position.x >= 0
        && a.position.y >= 0
        && a.hp <= hp
        && a.food_remaining <= food
        && (a.owner.is_none() || a.kind == AnimalKind::Sheep)
        && if a.hp == 0 {
            matches!(a.task, AnimalTask::Dead | AnimalTask::Carcass) && a.target.is_none()
        } else {
            a.food_remaining == food
                && match a.task {
                    AnimalTask::Idle => a.target.is_none(),
                    AnimalTask::Fleeing => a.kind == AnimalKind::Deer && a.target.is_none(),
                    AnimalTask::Attacking => {
                        matches!(a.kind, AnimalKind::Boar | AnimalKind::Wolf)
                            && a.target.is_some_and(|id| id != a.id)
                    }
                    AnimalTask::Dead | AnimalTask::Carcass => false,
                }
        }
}
pub(super) fn validate_server(message: &ServerMessage) -> Result<(), Error> {
    if let ServerMessage::WildlifeState { animals, .. } = message
        && (animals.len() > MAX_ANIMALS
            || !animals.iter().all(valid_animal)
            || !animals.windows(2).all(|p| p[0].id < p[1].id))
    {
        return Err(Error::InvalidEconomyState);
    }
    Ok(())
}
pub(super) fn validate_client(message: &ClientMessage) -> Result<(), Error> {
    if let ClientMessage::HuntOrder { unit_ids, .. } = message
        && (unit_ids.is_empty()
            || unit_ids.len() > economy::MAX_BUILD_GROUP
            || unit_ids.iter().collect::<BTreeSet<_>>().len() != unit_ids.len())
    {
        return Err(Error::InvalidEconomyState);
    }
    Ok(())
}
pub(super) fn bounded_animals<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<AnimalState>, D::Error> {
    economy::bounded::<D, _, MAX_ANIMALS>(d)
}
