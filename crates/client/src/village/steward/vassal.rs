use super::*;
use aoe_protocol::{VassalFocus, VassalJobs};

#[wasm_bindgen(
    inline_js = "export function vassal_update(v, label, jobs) { window.aoeVassal?.update(v, label, jobs); } export function vassal_selected() { return window.aoeVassal?.selected() ?? 0; }"
)]
extern "C" {
    fn vassal_update(v: &[u32], label: &str, jobs: &str);
    fn vassal_selected() -> u32;
}

fn occupants(client: &Client) -> impl Iterator<Item = &aoe_protocol::GarrisonUnitState> {
    super::super::production::selected(client)
        .filter(|b| b.player == owner(client) && b.kind == BuildingKind::Castle)
        .into_iter()
        .flat_map(|b| &b.garrison)
        .filter(|g| g.kind == UnitKind::Vassal && g.hp > 0 && g.vassal.is_some())
}

fn selected(client: &Client) -> Option<&aoe_protocol::GarrisonUnitState> {
    let mut occupants = occupants(client);
    let first = occupants.next()?;
    let requested = vassal_selected();
    if first.id.0 == requested {
        return Some(first);
    }
    occupants.find(|g| g.id.0 == requested).or(Some(first))
}

fn jobs_text(groups: [VassalJobs; 3]) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    for jobs in groups {
        for job in jobs.recent.into_iter().flatten() {
            let _ = write!(text, "{:?} at {}, {}\t", job.kind, job.tile.x, job.tile.y);
        }
        text.push('\n');
    }
    text
}

pub(super) fn present(client: &Client) {
    let Some((g, r)) = selected(client).and_then(|g| g.vassal.map(|r| (g, r))) else {
        vassal_update(&[], "", "");
        return;
    };
    let why = [
        "working",
        "waiting for economy reserve",
        "waiting for available builders",
        "site blocked or unreachable",
        "development paused",
        "development complete",
        "waiting for age",
    ]
    .get(usize::from(r.wait))
    .copied()
    .unwrap_or("waiting");
    let alert = if r.alert {
        format!(" · UNDER ATTACK · {} General(s) responding", r.responding)
    } else {
        "\nholding territory".to_owned()
    };
    let label = format!("{}/20 military{alert}\n{why}", r.military);
    let mut values = vec![
        g.id.0,
        u32::from(r.development),
        u32::from(r.alert),
        r.focus as u32,
        r.current.count,
        r.planned.count,
        r.finished.count,
    ];
    values.extend(occupants(client).map(|g| g.id.0));
    vassal_update(
        &values,
        &label,
        &jobs_text([r.current, r.planned, r.finished]),
    );
}

pub(super) fn hide() {
    vassal_update(&[], "", "");
}

/// Menu command 2 toggles development; 3 and 4 choose the Fortify or Roads focus.
pub(super) fn order(client: &mut Client, code: u32) {
    let Some((vassal_id, development)) =
        selected(client).and_then(|g| g.vassal.map(|r| (g.id, r.development)))
    else {
        return;
    };
    let sequence = client.next_sequence;
    use aoe_protocol::GameplayClientMessage as Message;
    let message = match code {
        2 => Message::VassalDevelopmentOrder {
            sequence,
            vassal_id,
            enabled: !development,
        },
        3 | 4 => Message::VassalFocusOrder {
            sequence,
            vassal_id,
            focus: if code == 3 {
                VassalFocus::Fortify
            } else {
                VassalFocus::Roads
            },
        },
        _ => return,
    };
    super::super::orders::send(client, message);
}
