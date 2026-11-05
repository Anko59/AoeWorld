//! HUD-cadence summaries of the bounded, authorized client stream. Never a
//! per-frame scan or a client-side reconstruction of General membership.
use super::super::{Client, village};
use aoe_core::PlayerId;
use aoe_protocol::{BuildingKind, GameplayBuildingState, GameplayUnitState, UnitKind};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(
    inline_js = "export function strategic_update(a, e, b, c, w, o, s, counts, sites, land, commerce) { const v = window.aoeNavigation?.strategic; v?.receive(a, e, b, c, w, o); v?.cockpit?.(s, counts); v?.settlements?.(sites); v?.holdings?.(land); v?.commerce?.(commerce); } export function strategic_alert(t, x, y) { window.aoeNavigation?.views?.alert(t, x, y); }"
)]
extern "C" {
    fn strategic_alert(tick: f64, x: f64, y: f64);
    fn strategic_update(
        armies: &[f64],
        economy: &[f64],
        bridges: &[f64],
        contacts: &[f64],
        workers: &[f64],
        orders: &[f64],
        summary: &[f64],
        counts: &[u32],
        sites: &[f64],
        land: &[f64],
        commerce: &[f64],
    );
}

pub(in super::super) fn site_buildings<'a>(
    buildings: &'a [GameplayBuildingState],
    owner: PlayerId,
    explored: impl Fn(&GameplayBuildingState) -> bool + 'a,
) -> impl Iterator<Item = &'a GameplayBuildingState> {
    [
        Some(BuildingKind::TownCenter),
        Some(BuildingKind::Castle),
        None,
    ]
    .into_iter()
    .flat_map(move |priority| {
        buildings.iter().filter(move |b| {
            let capital = b.player == owner
                && matches!(b.kind, BuildingKind::TownCenter | BuildingKind::Castle);
            priority.map_or(!capital, |kind| b.player == owner && b.kind == kind)
        })
    })
    .filter(move |b| b.hp > 0 && (b.player == owner || explored(b)))
    .take(2048)
}

pub(in super::super) fn site_record(
    b: &GameplayBuildingState,
    owner: PlayerId,
    colour: [u8; 3],
) -> [f64; 8] {
    let [r, g, blue] = colour.map(u32::from);
    [
        f64::from(b.id.0),
        f64::from(b.footprint.min.x),
        f64::from(b.footprint.min.y),
        f64::from(b.footprint.max.x),
        f64::from(b.footprint.max.y),
        b.kind as u8 as f64,
        f64::from(r << 16 | g << 8 | blue),
        f64::from(u8::from(b.player == owner)),
    ]
}

fn report_hit(hit: Option<aoe_protocol::HitState>, position: [f64; 2], newest: &mut [f64; 3]) {
    if let Some(hit) = hit {
        let tick = hit.tick.0 as f64;
        if !newest[0].is_finite() || tick > newest[0] {
            *newest = [tick, position[0], position[1]];
        }
    }
}

// [id, world x/y, owned, selected, size (-1 unknown), state, stance,
//  first authoritative order x/y (NaN absent), actual counts * 5].
pub(in super::super) fn army_record(
    u: &GameplayUnitState,
    owner: PlayerId,
    selected: bool,
    destination: Option<[f64; 2]>,
) -> Option<[f64; 15]> {
    if u.hp == 0 || u.kind != UnitKind::General {
        return None;
    }
    let owned = u.player == owner;
    let g = u.general.filter(|_| owned);
    let p = u.position.as_tiles();
    let target = destination.filter(|_| owned).unwrap_or([f64::NAN; 2]);
    let mut v = [0.0; 15];
    v[..10].copy_from_slice(&[
        f64::from(u.id.0),
        p[0],
        p[1],
        f64::from(u8::from(owned)),
        f64::from(u8::from(selected)),
        g.map_or(-1.0, |g| f64::from(g.army_size)),
        g.map_or(0.0, |g| g.state as u8 as f64),
        if owned { u.stance as u8 as f64 } else { -1.0 },
        target[0],
        target[1],
    ]);
    if let Some(g) = g {
        for (out, n) in v[10..].iter_mut().zip(g.counts) {
            *out = f64::from(n);
        }
    }
    Some(v)
}

pub(in super::super) fn update(client: &Client) {
    let owner = village::owner(client);
    let mut armies = Vec::new();
    let mut contacts = Vec::new();
    let mut workers = Vec::new();
    let mut last_alert = [f64::NAN; 3];
    let mut summary = vec![f64::NAN; 13];
    let player = client.village.players.iter().find(|p| p.player == owner);
    if let Some(p) = player {
        let s = p.stockpile;
        summary.copy_from_slice(&[
            f64::from(s.wood),
            f64::from(s.food),
            f64::from(s.gold),
            f64::from(s.stone),
            f64::from(p.population_used),
            f64::from(p.population_cap),
            p.recent_deliveries[1] as f64,
            p.recent_deliveries[0] as f64,
            p.recent_deliveries[2] as f64,
            p.recent_deliveries[3] as f64,
            p.held_tiles as f64,
            f64::from(p.held_share_ppm),
            f64::from(u8::from(client.village.holdings_complete)),
        ]);
    }
    for u in client.units.values() {
        let p = u.position.as_tiles();
        let own = u.player == owner;
        if own {
            report_hit(u.last_hit, p, &mut last_alert);
        }
        if !own && client.fog.state(p.map(|n| n.floor() as i32)) != 2 {
            continue;
        }
        if !own && contacts.len() < 1024 * 3 {
            let sighting = if u.hp > 0 { p } else { [f64::NAN; 2] };
            contacts.extend([f64::from(u.id.0), sighting[0], sighting[1]]);
        }
        if u.hp == 0 {
            continue;
        }
        if armies.len() < 256 * 15
            && let Some(v) = army_record(u, owner, client.selected.units.contains(&u.id), None)
        {
            armies.extend(v);
        }
        if own && (u.managed_by.is_some() || u.kind == UnitKind::Steward) {
            workers.extend([
                f64::from(u.id.0),
                u.managed_by.map_or(-1.0, |id| f64::from(id.0)),
                village::hud::worker_resource(u).map_or(-1.0, |kind| kind as u8 as f64),
                f64::from(u8::from(client.village.idle_villagers.contains(u.id))),
                u.home_castle
                    .filter(|_| u.kind == UnitKind::Steward)
                    .map_or(-1.0, |id| f64::from(id.0)),
            ]);
        }
    }
    let mut economy = Vec::new();
    let mut bridges = Vec::new();
    let sites: Vec<_> = site_buildings(&client.village.buildings, owner, |b| {
        client
            .fog
            .state(village::building_center(b).map(|n| n.floor() as i32))
            > 0
    })
    .flat_map(|b| {
        site_record(
            b,
            owner,
            aoe_rendering::player_colour(b.player, owner, client.team_colour),
        )
    })
    .collect();
    for b in &client.village.buildings {
        if b.player == owner && b.hp > 0 {
            report_hit(b.last_hit, village::building_center(b), &mut last_alert);
        }
        if matches!(b.kind, BuildingKind::Bridge | BuildingKind::Road) && bridges.len() < 2048 * 5 {
            bridges.extend([
                f64::from(b.footprint.min.x),
                f64::from(b.footprint.min.y),
                f64::from(b.footprint.max.x),
                f64::from(b.footprint.max.y),
                b.kind as u8 as f64,
            ]);
        }
        if b.player != owner || b.kind != BuildingKind::TownCenter || economy.len() >= 64 * 8 {
            continue;
        }
        let p = village::building_center(b);
        let garrison = b
            .garrison
            .iter()
            .find(|g| g.kind == UnitKind::Steward && g.hp > 0);
        let report = garrison.and_then(|g| g.steward);
        economy.extend([
            f64::from(b.id.0),
            p[0],
            p[1],
            f64::from(u8::from(client.selected.building_id == Some(b.id))),
            garrison.map_or(-1.0, |g| f64::from(g.id.0)),
            report.map_or(-1.0, |r| f64::from(r.managed)),
            report.map_or(-1.0, |r| f64::from(r.residents)),
            report.map_or(if garrison.is_some() { -2.0 } else { -1.0 }, |r| {
                r.wait as u8 as f64
            }),
        ]);
    }
    let mut orders = Vec::new();
    for q in client
        .village
        .move_queues
        .iter()
        .filter(|q| {
            client
                .units
                .get(&q.entity_id)
                .is_some_and(|u| u.kind == UnitKind::General && u.player == owner)
        })
        .take(256)
    {
        if let Some(p) = q.destinations.first() {
            let p = p.as_tiles();
            orders.extend([f64::from(q.entity_id.0), p[0], p[1]]);
        }
    }
    let land: Vec<f64> = client
        .village
        .holdings
        .iter()
        .flat_map(|s| {
            let [r, g, b] =
                aoe_rendering::player_colour(s.player, owner, client.team_colour).map(u32::from);
            [
                f64::from(s.x0),
                f64::from(s.y),
                f64::from(s.x1),
                f64::from(r << 16 | g << 8 | b),
            ]
        })
        .collect();
    strategic_update(
        &armies,
        &economy,
        &bridges,
        &contacts,
        &workers,
        &orders,
        &summary,
        player.map_or(&[], |p| p.unit_counts.as_slice()),
        &sites,
        &land,
        &village::building_panel::commerce(client),
    );
    strategic_alert(last_alert[0], last_alert[1], last_alert[2]);
}
