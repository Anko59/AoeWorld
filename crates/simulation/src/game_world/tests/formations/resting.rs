//! A large formation sent against a forest edge, where its packed files only
//! fit behind the goal, still comes to rest a body apart: members brush
//! through moving members only, so an arrival never settles on one at rest.
use super::*;
use aoe_core::Formation;

/// Pairs of members closer than half a militia's body width.
fn stacked_pairs(w: &GameWorld, ids: &[EntityId]) -> usize {
    let positions: Vec<_> = ids.iter().map(|id| w.unit(*id).unwrap().position).collect();
    let mut pairs = 0;
    for (n, a) in positions.iter().enumerate() {
        for b in &positions[n + 1..] {
            let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
            pairs += usize::from(dx * dx + dy * dy < 205 * 205);
        }
    }
    pairs
}

#[test]
fn a_hundred_members_sent_against_a_forest_come_to_rest_apart() {
    rest_against_a_forest(None);
}

/// An unseen squad steps as one body (no body sweeps) but still rests apart.
#[test]
fn an_unseen_hundred_sent_against_a_forest_come_to_rest_apart() {
    rest_against_a_forest(Some(PlayerId(1)));
}

fn rest_against_a_forest(watcher: Option<PlayerId>) {
    let mut w = wide_world();
    match watcher {
        Some(seat) => w.set_human_seats(&[seat]),
        None => w.set_fog_disabled(true),
    }
    let origin = center(30, 50);
    let ids = crate::game_world::formation::layout::slots(Formation::Box, 100)
        .into_iter()
        .map(|offset| {
            let p = WorldPosition::new(origin.x + offset.x, origin.y + offset.y);
            w.spawn_kind(PlayerId(0), UnitKind::Militia, p).unwrap()
        })
        .collect::<Vec<_>>();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for x in 72..90 {
            for y in 30..70 {
                let node = aoe_map::ResourceNode::starting(
                    TileCoord::new(x, y),
                    aoe_map::ResourceKind::Wood,
                )
                .unwrap();
                overlay.insert_starting_resource(generator, node).unwrap();
            }
        }
    }
    w.prepare_navigation(&|| false).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Box,
        center(71, 50),
        false,
        false,
    )
    .unwrap();
    for tick in 0..3000 {
        w.advance_movement();
        w.refill_route_budgets();
        if tick == 100 {
            let unseen = w.unseen_bystander(w.lookup[&ids[0]]);
            assert_eq!(
                unseen,
                watcher.is_some(),
                "the march is unseen exactly when watched from afar"
            );
        }
    }
    let moving = ids.iter().filter(|id| w.unit(**id).unwrap().moving).count();
    let stacked = stacked_pairs(&w, &ids);
    assert_eq!(moving, 0, "{watcher:?}");
    assert_eq!(
        stacked, 0,
        "members settled on members at rest ({watcher:?})"
    );
}

/// An unseen villager being chased keeps the full model like its pursuer;
/// one walking beside it, unchased, still strides.
#[test]
fn an_unseen_unit_being_attacked_is_not_a_bystander() {
    let mut w = wide_world();
    w.set_human_seats(&[PlayerId(1)]);
    let chased = w.spawn_villager(PlayerId(0), center(30, 50)).unwrap();
    let beside = w.spawn_villager(PlayerId(0), center(30, 54)).unwrap();
    let pursuer = w
        .spawn_kind(PlayerId(2), UnitKind::Militia, center(24, 50))
        .unwrap();
    w.prepare_navigation(&|| false).unwrap();
    w.issue_move(chased, center(70, 50)).unwrap();
    w.issue_move(beside, center(70, 54)).unwrap();
    w.issue_attack(PlayerId(2), pursuer, chased).unwrap();
    w.advance_movement();
    assert!(
        !w.unseen_bystander(w.lookup[&chased]),
        "the chased villager keeps the full model"
    );
    assert!(
        w.unseen_bystander(w.lookup[&beside]),
        "fixture: nobody watches"
    );
}
