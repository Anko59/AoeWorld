use super::*;
#[wasm_bindgen(
    inline_js = "export function vassal_update(v, label) { window.aoeVassal?.update(v, label); } export function vassal_bind(cb) { window.aoeVassal.bind(cb); }"
)]
extern "C" {
    fn vassal_update(v: &[u32], label: &str);
    #[wasm_bindgen(catch)]
    fn vassal_bind(cb: &js_sys::Function) -> Result<(), JsValue>;
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
        vassal_update(&[], "");
        return;
    };
    let Some(r) = g.vassal else {
        vassal_update(&[], "");
        return;
    };
    let why = [
        "building",
        "waiting for economy reserve (200 food, 200 wood, 100 gold, 200 stone)",
        "waiting for idle builders",
        "site blocked or unreachable; trying other sites",
        "development paused",
        "developed enough; spending stopped and builders released",
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
        "Vassal · {alert}{}/20 villagers · {}/20 military · next: {next} · {why}. Developed when two towers, a barracks and the gated wall ring are complete: {}/{} sites. Territory: 24 tiles. Calls Generals within 100 tiles.",
        r.villagers, r.military, r.completed, r.total
    );
    vassal_update(
        &[g.id.0, u32::from(r.development), u32::from(r.alert)],
        &label,
    );
}
pub(super) fn install(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback = Closure::<dyn FnMut()>::new(move || {
        let mut client = shared.borrow_mut();
        let Some((id, enabled)) =
            selected(&client).and_then(|g| g.vassal.map(|r| (g.id, !r.development)))
        else {
            return;
        };
        let sequence = client.next_sequence;
        super::super::orders::send(
            &mut client,
            aoe_protocol::GameplayClientMessage::VassalDevelopmentOrder {
                sequence,
                vassal_id: id,
                enabled,
            },
        );
    });
    vassal_bind(callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}
