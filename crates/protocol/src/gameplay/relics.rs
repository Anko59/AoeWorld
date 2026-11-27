use super::economy::bounded;
use aoe_core::{EntityId, PlayerId, WorldPosition};
use serde::{Deserialize, Deserializer, Serialize};

/// One local relic per seat at geographic scale, with room for neutral sites.
pub const MAX_RELICS: usize = 2_048;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RelicAction {
    PickUp { monk: EntityId, relic: EntityId },
    Drop { monk: EntityId },
    Eject { monastery: EntityId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroundRelic {
    pub id: EntityId,
    pub position: WorldPosition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MonasteryRelics {
    pub id: EntityId,
    pub count: u16,
    pub full: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelicVictory {
    pub leader: PlayerId,
    pub remaining_seconds: u16,
}

#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelicView {
    pub revision: u64,
    #[serde(deserialize_with = "bounded_relics")]
    pub ground: Vec<GroundRelic>,
    #[serde(deserialize_with = "bounded_relics")]
    pub carriers: Vec<EntityId>,
    #[serde(deserialize_with = "bounded_relics")]
    pub monasteries: Vec<MonasteryRelics>,
    pub victory: Option<RelicVictory>,
}

impl RelicView {
    pub(super) fn valid(&self) -> bool {
        self.revision > 0
            && self
                .ground
                .iter()
                .all(|r| r.position.x >= 0 && r.position.y >= 0)
            && self.ground.windows(2).all(|x| x[0].id < x[1].id)
            && self.carriers.windows(2).all(|x| x[0] < x[1])
            && self.monasteries.windows(2).all(|x| x[0].id < x[1].id)
            && self.monasteries.iter().all(|m| m.count > 0)
            && self.ground.len()
                + self.carriers.len()
                + self
                    .monasteries
                    .iter()
                    .map(|m| usize::from(m.count))
                    .sum::<usize>()
                <= MAX_RELICS
    }
}

fn bounded_relics<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    bounded::<D, _, MAX_RELICS>(d)
}
