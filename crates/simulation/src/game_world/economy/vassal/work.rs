//! Foundation progress reported by the castle, independent of who builds it.
use super::{plan::Site, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum VassalFocus {
    #[default]
    Fortify,
    Roads,
}

impl VassalFocus {
    pub(super) fn priority(self, kind: BuildingKind) -> bool {
        self == Self::Roads && !matches!(kind, BuildingKind::Road | BuildingKind::Bridge)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VassalJob {
    pub kind: BuildingKind,
    pub tile: TileCoord,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VassalJobs {
    pub count: u32,
    pub recent: [Option<VassalJob>; 3],
}

impl VassalJobs {
    fn preview(&mut self, job: VassalJob) {
        if let Some(slot) = self.recent.get_mut(self.count as usize) {
            *slot = Some(job);
        }
        self.count += 1;
    }

    fn finished(&mut self, job: VassalJob) {
        self.recent.rotate_right(1);
        self.recent[0] = Some(job);
        self.count = self.count.saturating_add(1);
    }
}

#[derive(Default)]
pub(super) struct Workload {
    pub(super) deficits: Vec<(usize, Site)>,
    pub(super) open: Vec<(EntityId, VassalJob)>,
    complete: usize,
    total: usize,
}

impl GameWorld {
    pub fn issue_vassal_focus(
        &mut self,
        player: PlayerId,
        id: EntityId,
        focus: VassalFocus,
    ) -> Result<(), EconomyError> {
        self.owned_vassal(player, id)?.status.focus = focus;
        Ok(())
    }

    pub(super) fn vassal_workload(&self, d: &Domain, sites: &[Site]) -> Workload {
        let mut work = Workload {
            total: sites.len(),
            ..Workload::default()
        };
        let mut counted = BTreeSet::new();
        for (index, &site) in sites.iter().enumerate() {
            let building = self
                .building_at_for(d.player, site.tile)
                .and_then(|id| self.building(id))
                .filter(|b| {
                    b.player == d.player
                        && b.hp > 0
                        && (b.kind == site.kind
                            || b.is_complete() && serves_site(b.kind, site.kind))
                });
            match building.as_deref() {
                Some(b) if b.is_complete() => work.complete += 1,
                Some(b) if counted.insert(b.id) => work.open.push((b.id, job(b))),
                Some(_) => (),
                None => work.deficits.push((index, site)),
            }
        }
        for &id in &d.paid {
            if let Some(b) = self
                .building(id)
                .filter(|b| b.player == d.player && !b.is_complete() && b.hp > 0)
                && counted.insert(id)
            {
                work.open.push((id, job(&b)));
            }
        }
        work
    }

    pub(super) fn publish_vassal_work(&mut self, id: EntityId, mut work: Workload) {
        let focus = self.economy.vassals.domains[&id].status.focus;
        let mut current = VassalJobs::default();
        let mut planned = VassalJobs::default();
        work.deficits
            .sort_by_key(|(_, site)| (focus.priority(site.kind), site.tier));
        for (building, job) in work.open {
            if self.economy.builders.assigned_count(building) == 0 {
                planned.preview(job);
            } else {
                current.preview(job);
            }
        }
        for (_, site) in work.deficits {
            planned.preview(VassalJob {
                kind: site.kind,
                tile: site.tile,
            });
        }
        let Some(domain) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        let status = &mut domain.status;
        status.completed = u16::try_from(work.complete).unwrap_or(u16::MAX);
        status.total = u16::try_from(work.total).unwrap_or(u16::MAX);
        status.planned = planned;
        status.current = current;
        if status.wait == VassalWait::Building && current.count == 0 {
            status.wait = VassalWait::Builders;
        }
    }

    /// A placed job gets credit once, whoever completed it.
    pub(in super::super) fn finish_vassal_job(&mut self, vassal: EntityId, building: EntityId) {
        let Some(job) = self.building(building).map(|b| job(&b)) else {
            return;
        };
        if let Some(d) = self.economy.vassals.domains.get_mut(&vassal) {
            d.status.finished.finished(job);
        }
    }
}

fn serves_site(actual: BuildingKind, planned: BuildingKind) -> bool {
    match planned {
        BuildingKind::WatchTower => actual.is_tower(),
        BuildingKind::PalisadeWall => actual == BuildingKind::StoneWall || actual.is_gate(),
        BuildingKind::PalisadeGate | BuildingKind::StoneWall => actual == BuildingKind::StoneGate,
        _ => false,
    }
}

fn job(b: &crate::GameBuilding) -> VassalJob {
    VassalJob {
        kind: b.kind,
        tile: b.footprint.min,
    }
}
