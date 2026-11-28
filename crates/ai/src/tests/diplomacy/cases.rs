use crate::tests::{
    fake::{FakeWorld, base},
    play::play_fresh,
};
use aoe_sdk::{AllianceOffer, DiplomacySeat, DiplomacyView, DiplomaticStance, Order, PlayerId};

#[test]
fn brain_accepts_an_incoming_alliance_through_the_ordinary_order_path() {
    let (units, buildings) = base(6);
    let mut world = FakeWorld::new(units, buildings);
    world.diplomacy = Some(DiplomacyView {
        revision: 1,
        seats: vec![
            DiplomacySeat {
                player: PlayerId(1),
                name: None,
                stance: DiplomaticStance::Enemy,
                offer: AllianceOffer::Received,
            },
            DiplomacySeat {
                player: PlayerId(2),
                name: None,
                stance: DiplomaticStance::Enemy,
                offer: AllianceOffer::Sent,
            },
        ],
        ..DiplomacyView::default()
    });
    let (_, recorder) = play_fresh(&world);
    let offers: Vec<_> = recorder
        .orders
        .iter()
        .filter_map(|order| match order {
            Order::Diplomacy {
                player,
                stance: DiplomaticStance::Ally,
            } => Some(*player),
            _ => None,
        })
        .collect();
    assert_eq!(offers, [PlayerId(1)]);
}

#[test]
fn brain_dispatches_soldiers_to_a_defence_request_and_replies() {
    use aoe_sdk::{DiplomacyEntry, DiplomacyMessage, TileCoord, UnitKind};
    let (mut units, buildings) = base(6);
    let mut soldier = units[0].clone();
    soldier.id = 900;
    soldier.kind = UnitKind::Militia;
    soldier.military = true;
    units.push(soldier);
    let mut world = FakeWorld::new(units, buildings);
    let ally = PlayerId(1);
    let viewer = PlayerId(0);
    let at = TileCoord::new(41, 42);
    world.diplomacy = Some(DiplomacyView {
        revision: 1,
        viewer,
        seats: vec![DiplomacySeat {
            player: ally,
            name: None,
            stance: DiplomaticStance::Ally,
            offer: AllianceOffer::None,
        }],
        log: vec![DiplomacyEntry {
            id: 1,
            from: ally,
            to: viewer,
            message: DiplomacyMessage::Defend(at),
        }],
        ..Default::default()
    });
    let (mut brain, recorder) = play_fresh(&world);
    let mission = recorder.orders.iter().position(|order| matches!(order, Order::FormationMove { units, to, attack_move: true, .. } if *to == at && units.iter().any(|id| id.0 == 900))).expect("defending formation");
    let answer = Order::DiplomacyMessage {
        player: ally,
        message: DiplomacyMessage::Reply {
            request: 1,
            accepted: true,
        },
    };
    assert!(!recorder.orders.contains(&answer));
    let recorder = crate::tests::play::play(
        &world,
        &mut brain,
        &[aoe_sdk::Event::OrderResult {
            sequence: mission as u64 + 1,
            result: aoe_sdk::CommandResult::Accepted,
        }],
    );
    assert!(recorder.orders.contains(&answer));
}

#[test]
fn threatened_brain_asks_a_known_ally_to_defend_or_offers_a_new_alliance() {
    use aoe_sdk::{DiplomacyMessage, TileCoord};
    let (units, buildings) = base(6);
    let home = buildings[0].center();
    let mut world = FakeWorld::new(units, buildings);
    let ally = PlayerId(1);
    let enemy = PlayerId(2);
    world.enemies = vec![
        crate::view::Unit {
            mine: false,
            owner: enemy.0,
            attack_target: Some(1),
            ..crate::tests::fake::soldier(800, home)
        },
        crate::view::Unit {
            mine: false,
            owner: ally.0,
            ..crate::tests::fake::soldier(801, home)
        },
    ];
    let (_, recorder) = play_fresh(&world);
    assert!(recorder.orders.contains(&Order::Diplomacy {
        player: ally,
        stance: DiplomaticStance::Ally,
    }));

    world.enemies.pop();
    world.diplomacy = Some(DiplomacyView {
        revision: 1,
        seats: vec![DiplomacySeat {
            player: ally,
            name: None,
            stance: DiplomaticStance::Ally,
            offer: AllianceOffer::None,
        }],
        ..Default::default()
    });
    let (_, recorder) = play_fresh(&world);
    assert!(recorder.orders.contains(&Order::DiplomacyMessage {
        player: ally,
        message: DiplomacyMessage::Defend(TileCoord::new(home.0, home.1)),
    }));
}
