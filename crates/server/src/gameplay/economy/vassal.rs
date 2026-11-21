use super::wire_building;

pub(super) fn wire_vassal(v: aoe_simulation::VassalStatus) -> aoe_protocol::VassalReport {
    let jobs = |jobs: aoe_simulation::VassalJobs| aoe_protocol::VassalJobs {
        count: jobs.count,
        recent: jobs.recent.map(|job| {
            job.filter(|job| job.tile.checked().is_ok())
                .map(|job| aoe_protocol::VassalJob {
                    kind: wire_building(job.kind),
                    tile: job.tile,
                })
        }),
    };
    let focus = match v.focus {
        aoe_simulation::VassalFocus::Fortify => aoe_protocol::VassalFocus::Fortify,
        aoe_simulation::VassalFocus::Roads => aoe_protocol::VassalFocus::Roads,
    };
    aoe_protocol::VassalReport {
        military: v.military,
        development: v.development,
        developed: v.developed,
        alert: v.alert,
        next: v.next.map(wire_building),
        wait: v.wait as u8,
        completed: v.completed,
        total: v.total,
        responding: v.responding,
        focus,
        current: jobs(v.current),
        planned: jobs(v.planned),
        finished: jobs(v.finished),
    }
}
