//! Unit deltas against a session's delivered baseline.
use super::{Built, Resident};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::GameplayUnitState;

/// What changed between a session's baseline and the units now in view.
pub(super) struct UnitDiff {
    pub(super) changed: Vec<GameplayUnitState>,
    pub(super) removals: Vec<EntityId>,
    /// The same ids are in view, so the baseline can be patched in place.
    same_ids: bool,
}

/// Only units the frame marks changed are compared: one the baseline holds
/// from the frame before is otherwise equal to it by construction.
pub(super) fn diff(
    resident: &Resident,
    current: &[u32],
    built: &Built,
    viewer: PlayerId,
) -> UnitDiff {
    let mut diff = UnitDiff {
        changed: Vec::new(),
        removals: Vec::new(),
        same_ids: true,
    };
    let frame = &built.frame;
    let mut old = resident.units.iter().peekable();
    for slot in current {
        let state = frame.state(*slot);
        while let Some(before) = old.next_if(|before| before.id < state.id) {
            diff.removals.push(before.id);
            diff.same_ids = false;
        }
        match old.next_if(|before| before.id == state.id) {
            Some(_) if frame.unchanged_since(resident.frame, *slot) => {}
            Some(before) => {
                let now = state.for_viewer(viewer);
                if before != &now {
                    diff.changed.push(now);
                }
            }
            None => {
                diff.changed.push(state.for_viewer(viewer));
                diff.same_ids = false;
            }
        }
    }
    for before in old {
        diff.removals.push(before.id);
        diff.same_ids = false;
    }
    diff
}

pub(super) fn patch(
    resident: &mut Resident,
    diff: &UnitDiff,
    current: &[u32],
    built: &Built,
    viewer: PlayerId,
) {
    if diff.same_ids {
        // Both lists are in id order (the diff walks the view in order):
        // one forward cursor finds every changed unit, no search per unit.
        let mut at = 0;
        for unit in &diff.changed {
            while at < resident.len() && resident[at].id < unit.id {
                at += 1;
            }
            if let Some(slot) = resident.units.get_mut(at).filter(|u| u.id == unit.id) {
                *slot = *unit;
            }
        }
    } else {
        resident.units = current
            .iter()
            .map(|slot| built.frame.state(*slot).for_viewer(viewer))
            .collect();
    }
    resident.frame = Some(built.frame.generation());
}
