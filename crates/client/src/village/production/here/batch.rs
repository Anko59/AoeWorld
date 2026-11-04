//! Rally admission precedes training; only accepted training acknowledgements count.
use aoe_core::EntityId;
use aoe_protocol::{CommandResult, UnitKind};

#[derive(Default)]
pub(crate) struct Batch {
    pub(super) kind: Option<UnitKind>,
    pub(super) rallies: Vec<(u64, EntityId, usize)>,
    pub(super) trains: Vec<u64>,
    pub(super) accepted: usize,
}

pub(super) enum Ack {
    Rally(EntityId, usize),
    Training,
}

impl Batch {
    pub(super) fn new(kind: UnitKind) -> Self {
        Self {
            kind: Some(kind),
            ..Self::default()
        }
    }

    pub(super) fn active(&self) -> bool {
        !self.rallies.is_empty() || !self.trains.is_empty()
    }

    pub(super) fn acknowledge(&mut self, sequence: u64, result: CommandResult) -> Option<Ack> {
        if let Some(at) = self.rallies.iter().position(|entry| entry.0 == sequence) {
            let (_, id, count) = self.rallies.remove(at);
            return Some(Ack::Rally(
                id,
                if result == CommandResult::Accepted {
                    count
                } else {
                    0
                },
            ));
        }
        let at = self.trains.iter().position(|seq| *seq == sequence)?;
        self.trains.remove(at);
        self.accepted += usize::from(result == CommandResult::Accepted);
        Some(Ack::Training)
    }
}

#[cfg(test)]
#[path = "../tests/batch.rs"]
mod tests;
