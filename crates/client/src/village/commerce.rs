//! Far-view commerce report, planned from every owned building and independent
//! of the building selection.
use super::{Client, construction, owner, production};
use crate::building_ui::{self, availability};
use aoe_core::PlayerId;
use aoe_protocol::{
    BuildingKind, ConstructionState, GameplayBuildingState, GameplayPlayerState, UnitKind,
};

fn busy(b: &GameplayBuildingState) -> bool {
    b.research.is_some() || !b.training_queue.is_empty()
}

/// Market id (-1 none), wood/food/stone prices, then per technology its
/// availability code and the building that would research it (-1 none), then
/// per unit kind (`UnitKind` order) the best availability code over all owned
/// buildings that can train it: 1 when one could train it now, 12 when no owned
/// building can. One candidate per building kind keeps the research part bounded
/// by the technology table, not by how many buildings a kingdom owns.
pub(super) fn report(
    buildings: &[GameplayBuildingState],
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    completed: u32,
) -> Vec<f64> {
    let mut market = None;
    let mut researching: aoe_protocol::TechMask = 0;
    let mut candidate: [Option<&GameplayBuildingState>; 32] = [None; 32];
    for b in buildings {
        if b.player != owner || b.hp == 0 {
            continue;
        }
        if let Some(r) = &b.research {
            researching |= r.tech.bit();
        }
        if b.construction != ConstructionState::Complete {
            continue;
        }
        if b.kind == BuildingKind::Market && market.is_none() {
            market = Some(b.id);
        }
        // An idle building beats a busy one: research is refused while training.
        let slot = &mut candidate[(b.kind as usize) & 31];
        if (*slot).is_none_or(|c| busy(c) && !busy(b)) {
            *slot = Some(b);
        }
    }
    let prices = player.map_or(aoe_protocol::MarketPrices::default(), |p| p.market_prices);
    let mut v = vec![
        market.map_or(-1.0, |id| f64::from(id.0)),
        f64::from(prices.wood),
        f64::from(prices.food),
        f64::from(prices.stone),
    ];
    for r in building_ui::content::rules().techs.iter() {
        let duplicate = researching & (1 << r.tech) != 0;
        match candidate[(r.building as usize) & 31] {
            Some(b) => {
                v.push(availability::research(b, player, owner, completed, duplicate, r).number());
                v.push(f64::from(b.id.0));
            }
            None => v.extend([availability::Availability::Unavailable.number(), -1.0]),
        }
    }
    for kind in UnitKind::ALL {
        v.push(unit_code(buildings, player, owner, kind));
    }
    v
}

fn unit_code(
    buildings: &[GameplayBuildingState],
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    kind: UnitKind,
) -> f64 {
    use availability::Availability::{Construction, Ready, Requirement, Unavailable};
    let mut best: Option<(bool, bool, u8)> = None;
    for b in buildings {
        if b.player != owner || b.hp == 0 {
            continue;
        }
        let code = if kind == UnitKind::Steward && production::has_steward(b) {
            availability::Availability::Unavailable
        } else {
            availability::training(b, player, owner, kind)
        };
        if matches!(code, Ready) {
            return Ready.number();
        }
        if !building_ui::rules(b.kind).trains(kind as u8) {
            continue;
        }
        // Prefer a reason that names the real obstacle over "unavailable" or "still building".
        let rank = (
            matches!(code, Unavailable),
            matches!(code, Construction),
            code as u8,
        );
        best = Some(best.map_or(rank, |old| old.min(rank)));
    }
    best.map_or(Requirement.number(), |(_, _, code)| f64::from(code))
}

#[inline(never)]
pub(in super::super) fn commerce(client: &Client) -> Vec<f64> {
    let owner = owner(client);
    let player = client.village.players.iter().find(|p| p.player == owner);
    report(
        &client.village.buildings,
        player,
        owner,
        construction::completed(client),
    )
}

/// Right-clicking an own, complete Market or Dock with Trade Carts or Cogs
/// selected sends them shuttling between it and the farthest other one.
pub(super) fn trade_interact(client: &mut Client, point: aoe_core::ScreenPoint) -> bool {
    let Some(b) = construction::target(client, point) else {
        return false;
    };
    let (building_id, kind) = (b.id, b.kind);
    if b.player != owner(client)
        || b.construction != aoe_protocol::ConstructionState::Complete
        || !matches!(
            kind,
            aoe_protocol::BuildingKind::Market | aoe_protocol::BuildingKind::Dock
        )
    {
        return false;
    }
    let ids: Vec<_> = client
        .selected
        .units
        .iter()
        .copied()
        .filter(|id| {
            client.units.get(id).is_some_and(|u| {
                u.player == owner(client)
                    && u.hp > 0
                    && match u.kind {
                        UnitKind::TradeCart => kind == aoe_protocol::BuildingKind::Market,
                        UnitKind::TradeCog => kind == aoe_protocol::BuildingKind::Dock,
                        _ => false,
                    }
            })
        })
        .collect();
    for &entity_id in &ids {
        let sequence = client.next_sequence;
        super::orders::send(
            client,
            aoe_protocol::GameplayClientMessage::TradeOrder {
                sequence,
                entity_id,
                building_id,
            },
        );
    }
    !ids.is_empty()
}
