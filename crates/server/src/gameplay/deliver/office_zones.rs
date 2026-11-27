use super::{Session, SessionBuild, send};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::GameplayServerMessage;
use aoe_simulation::{GameWorld, OfficeZoneChunk};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(in crate::gameplay) type OfficeZones = BTreeMap<EntityId, (bool, Arc<[OfficeZoneChunk]>)>;

pub(in crate::gameplay) fn view(world: &GameWorld, viewer: PlayerId, ai: bool) -> OfficeZones {
    if ai {
        return BTreeMap::new();
    }
    [false, true]
        .into_iter()
        .flat_map(|vassal| {
            world
                .office_zones(viewer, vassal)
                .map(move |(id, shape)| (id, (vassal, shape)))
        })
        .collect()
}

#[derive(Debug, Default)]
pub(in crate::gameplay) struct Delivery {
    offices: BTreeMap<EntityId, Progress>,
    primed: bool,
}

#[derive(Debug)]
struct Progress {
    shape: Arc<[OfficeZoneChunk]>,
    sent: usize,
}

pub(super) fn deliver(session: &mut Session, build: &SessionBuild, failed: &mut BTreeSet<u64>) {
    let mut state = std::mem::take(&mut session.office_zones);
    state.send(session, build, failed);
    session.office_zones = state;
}

impl Delivery {
    /// A new socket receives its zones with the first snapshot, also while
    /// paused. The subscription queue always drains this initial view in full.
    pub(in crate::gameplay) fn initial(
        &mut self,
        world: &GameWorld,
        viewer: PlayerId,
        ai: bool,
    ) -> impl Iterator<Item = GameplayServerMessage> + use<> {
        let shapes = if std::mem::replace(&mut self.primed, true) {
            BTreeMap::new()
        } else {
            view(world, viewer, ai)
        };
        self.offices
            .extend(shapes.iter().map(|(&office, (_, shape))| {
                (
                    office,
                    Progress {
                        shape: shape.clone(),
                        sent: shape.len(),
                    },
                )
            }));
        shapes.into_iter().flat_map(|(office, (vassal, shape))| {
            let mut offset = 0;
            std::iter::from_fn(move || {
                let (message, next) =
                    GameplayServerMessage::office_zone_batch(office, vassal, &shape, offset)?;
                offset = next;
                Some(message)
            })
        })
    }

    fn send(&mut self, session: &mut Session, build: &SessionBuild, failed: &mut BTreeSet<u64>) {
        let removed = self
            .offices
            .keys()
            .filter(|id| !build.office_zones.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for office in removed {
            if !send(
                session,
                GameplayServerMessage::OfficeZone {
                    office,
                    vassal: false,
                    start: true,
                    complete: true,
                    chunks: Vec::new(),
                },
                failed,
                build.session_id,
            ) {
                return;
            }
            self.offices.remove(&office);
        }
        for (&office, (vassal, shape)) in &build.office_zones {
            let progress = self.offices.entry(office).or_insert_with(|| Progress {
                shape: shape.clone(),
                sent: 0,
            });
            if !Arc::ptr_eq(&progress.shape, shape) {
                if progress.shape != *shape {
                    progress.sent = 0;
                }
                progress.shape = shape.clone();
            }
            // Socket capacity limits catch-up. Only advance a piece once
            // it fits, so the next tick can finish a stalled transfer.
            while let Some((message, next)) =
                GameplayServerMessage::office_zone_batch(office, *vassal, shape, progress.sent)
            {
                if !send(session, message, failed, build.session_id) {
                    return;
                }
                progress.sent = next;
            }
        }
    }
}
