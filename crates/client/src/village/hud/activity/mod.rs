//! Kingdom-wide production summary from the client's existing owned buildings.
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{GameplayBuildingState, Tech, UnitKind};

pub(crate) const MAX_ROWS: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    pub research: bool,
    pub kind: u8,
    pub count: u32,
    pub progress: u16,
    pub building: EntityId,
    active: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Summary {
    pub entries: Vec<Entry>,
    pub more: usize,
}

/// Fixed type-indexed storage, independent of the number of buildings. Queued
/// units count, but only a queue's head advances its type's progress bar.
/// Show the furthest advanced producer; ties choose the lowest entity id.
pub(crate) fn summarize(buildings: &[GameplayBuildingState], owner: PlayerId) -> Summary {
    let mut groups: [Option<Entry>; UnitKind::ALL.len() + Tech::ALL.len()] =
        [None; UnitKind::ALL.len() + Tech::ALL.len()];
    let mut add =
        |research: bool, kind: u8, progress: u64, total: u64, building: EntityId, active: bool| {
            let slot = usize::from(kind) + if research { UnitKind::ALL.len() } else { 0 };
            let progress =
                (u128::from(progress.min(total)) * 1000 / u128::from(total.max(1))) as u16;
            let entry = groups[slot].get_or_insert(Entry {
                research,
                kind,
                count: 0,
                progress,
                building,
                active,
            });
            entry.count += 1;
            if (active, progress) > (entry.active, entry.progress)
                || (active, progress) == (entry.active, entry.progress) && building < entry.building
            {
                entry.progress = progress;
                entry.building = building;
                entry.active = active;
            }
        };
    for b in buildings.iter().filter(|b| {
        b.player == owner && b.hp > 0 && b.construction == aoe_protocol::ConstructionState::Complete
    }) {
        for (i, queued) in b.training_queue.iter().enumerate() {
            add(
                false,
                queued.kind as u8,
                if i == 0 { queued.progress_ticks } else { 0 },
                queued.total_ticks,
                b.id,
                i == 0,
            );
        }
        if let Some(r) = b.research {
            add(
                true,
                r.tech as u8,
                r.progress_ticks,
                r.total_ticks,
                b.id,
                true,
            );
        }
    }
    let count = groups.iter().flatten().count();
    Summary {
        entries: groups.into_iter().flatten().take(MAX_ROWS).collect(),
        more: count.saturating_sub(MAX_ROWS),
    }
}

#[cfg(test)]
mod tests;
