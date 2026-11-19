use aoe_protocol::UnitKind;

pub(crate) struct ArmyKinds {
    counts: [u16; UnitKind::ALL.len()],
    total: u16,
}

impl Default for ArmyKinds {
    fn default() -> Self {
        Self {
            counts: [0; UnitKind::ALL.len()],
            total: 0,
        }
    }
}

impl ArmyKinds {
    pub(crate) fn add(&mut self, kind: UnitKind) {
        self.counts[kind as usize] += 1;
        self.total += 1;
    }

    pub(crate) fn complete_dominant(&self, size: u16) -> Option<UnitKind> {
        if size == 0 || self.total != size {
            return None;
        }
        UnitKind::ALL
            .into_iter()
            .rev()
            .max_by_key(|kind| self.counts[*kind as usize])
    }
}
