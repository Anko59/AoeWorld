use super::*;
#[wasm_bindgen(
    inline_js = "export function vassal_update(v, label, help) { window.aoeVassal?.update(v, label, help); }"
)]
extern "C" {
    fn vassal_update(v: &[u32], label: &str, help: &str);
}
fn selected(client: &Client) -> Option<&aoe_protocol::GarrisonUnitState> {
    super::super::production::selected(client)
        .filter(|b| b.player == owner(client) && b.kind == BuildingKind::Castle)?
        .garrison
        .iter()
        .find(|g| g.kind == UnitKind::Vassal && g.hp > 0)
}
pub(super) fn present(client: &Client) {
    let Some(g) = selected(client) else {
        hide();
        return;
    };
    let Some(r) = g.vassal else {
        hide();
        return;
    };
    let why = [
        "building",
        "waiting for reserves",
        "waiting for idle builders",
        "trying alternate sites",
        "development paused",
        "development complete",
        "waiting for age",
    ]
    .get(usize::from(r.wait))
    .copied()
    .unwrap_or("waiting");
    let next = r
        .next
        .map_or_else(|| "none".to_owned(), |kind| format!("{kind:?}"));
    let alert = if r.alert {
        format!("UNDER ATTACK · {} General(s) responding · ", r.responding)
    } else {
        "holding territory · ".into()
    };
    let label = format!(
        "Vassal · {alert}{}/20 villagers · {}/20 military · next: {next} · {why} · {}/{} sites",
        r.villagers, r.military, r.completed, r.total
    );
    vassal_update(
        &[g.id.0, u32::from(r.development), u32::from(r.alert)],
        &label,
        "Development requires two towers, a barracks, a gated wall ring and a road to the nearest Town Center. Economy reserve: 200 food, 200 wood, 100 gold and 200 stone. Territory: 24 tiles. Calls Generals within 100 tiles.",
    );
}
pub(super) fn hide() {
    vassal_update(&[], "", "");
}
pub(super) fn toggle(client: &mut Client) {
    let Some((id, enabled)) =
        selected(client).and_then(|g| g.vassal.map(|r| (g.id, !r.development)))
    else {
        return;
    };
    let sequence = client.next_sequence;
    super::super::orders::send(
        client,
        aoe_protocol::GameplayClientMessage::VassalDevelopmentOrder {
            sequence,
            vassal_id: id,
            enabled,
        },
    );
}
