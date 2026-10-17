use aoe_core::{EntityId, PlayerId, ScreenPoint};
use aoe_protocol::UnitKind;

#[derive(Default)]
pub(crate) struct Selection {
    pub units: Vec<EntityId>,
    pub resource_id: Option<u64>,
    pub building_id: Option<EntityId>,
}
impl Selection {
    pub fn building(&mut self, id: EntityId) {
        self.resource_id = None;
        self.units.clear();
        self.building_id = Some(id);
    }
    pub fn units(&mut self, units: Vec<EntityId>) {
        self.resource_id = None;
        self.units = units;
        self.building_id = None;
    }
    pub fn resource(&mut self, id: u64) {
        self.units.clear();
        self.building_id = None;
        self.resource_id = Some(id);
    }
    pub fn same_kind(
        &mut self,
        candidates: impl IntoIterator<Item = (EntityId, PlayerId, UnitKind, bool)>,
        owner: PlayerId,
        kind: UnitKind,
    ) {
        self.units(
            candidates
                .into_iter()
                .filter_map(|(id, player, candidate, visible)| {
                    (player == owner && candidate == kind && visible).then_some(id)
                })
                .collect(),
        );
    }
}

pub(crate) fn box_members(
    candidates: impl IntoIterator<Item = (EntityId, PlayerId, ScreenPoint)>,
    owner: PlayerId,
    start: ScreenPoint,
    end: ScreenPoint,
) -> Vec<EntityId> {
    candidates
        .into_iter()
        .filter_map(|(id, player, p)| {
            (player == owner
                && p.x >= start.x.min(end.x)
                && p.x <= start.x.max(end.x)
                && p.y >= start.y.min(end.y)
                && p.y <= start.y.max(end.y))
            .then_some(id)
        })
        .collect()
}
