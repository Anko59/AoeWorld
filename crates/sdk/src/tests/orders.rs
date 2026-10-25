use crate::{BuildingKind, Order, Tech, TradeResource, UnitKind};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_protocol::{GameplayClientMessage as M, decode_gameplay_client, encode_gameplay_client};

fn id(n: u32) -> EntityId {
    EntityId(n)
}

fn all_orders() -> Vec<Order> {
    let tile = TileCoord::new(10, 20);
    vec![
        Order::AutoScout {
            unit: id(1),
            enabled: true,
        },
        Order::Move {
            unit: id(1),
            to: tile,
        },
        Order::MoveTo {
            unit: id(1),
            to: WorldPosition::new(5_000, 6_000),
        },
        Order::Stop { unit: id(1) },
        Order::QueueMove {
            unit: id(1),
            to: tile,
        },
        Order::Gather {
            unit: id(2),
            resource: 77,
        },
        Order::Build {
            builders: vec![id(3), id(4)],
            kind: BuildingKind::House,
            origin: tile,
        },
        Order::BuildWall {
            builders: vec![id(3)],
            from: tile,
            to: TileCoord::new(14, 20),
        },
        Order::Construct {
            builders: vec![id(3)],
            building: id(9),
        },
        Order::CancelConstruction { building: id(9) },
        Order::Farm {
            unit: id(5),
            farm: id(6),
        },
        Order::FarmReseed {
            building: id(7),
            add: true,
        },
        Order::Train {
            building: id(7),
            kind: UnitKind::Spearman,
        },
        Order::CancelTraining { building: id(7) },
        Order::SetRally {
            building: id(7),
            target: crate::RallyTarget::Ground(WorldPosition::new(512, 1024)),
        },
        Order::SetRally {
            building: id(7),
            target: crate::RallyTarget::Resource(77),
        },
        Order::SetRally {
            building: id(7),
            target: crate::RallyTarget::Building(id(9)),
        },
        Order::SetRally {
            building: id(7),
            target: crate::RallyTarget::Animal(id(12)),
        },
        Order::Research {
            building: id(7),
            tech: Tech::FeudalAge,
        },
        Order::CancelResearch { building: id(7) },
        Order::Attack {
            unit: id(8),
            target: id(99),
        },
        Order::Hunt {
            units: vec![id(2), id(3)],
            animal: id(55),
        },
        Order::MarketBuy {
            building: id(10),
            resource: TradeResource::Wood,
        },
        Order::MarketSell {
            building: id(10),
            resource: TradeResource::Stone,
        },
    ]
}

#[test]
fn every_order_maps_to_a_wire_message_that_round_trips() {
    let orders = all_orders();
    let mut seen = std::collections::BTreeSet::new();
    for (index, order) in orders.iter().enumerate() {
        let sequence = index as u64 + 1;
        let message = order.message(sequence).expect("encodable");
        let bytes = encode_gameplay_client(&message).unwrap();
        assert_eq!(decode_gameplay_client(&bytes).unwrap(), message);
        seen.insert(bytes[0]);
    }
    // Move, Stop, Gather, Build, BuildWall, Construct, CancelConstruction, Farm,
    // Training, construction, combat, rally, queued moves, reseeds and auto-scout
    // all retain distinct ordinary authenticated wire variants.
    assert_eq!(seen.len(), 20, "distinct wire variants: {seen:?}");
}

#[test]
fn a_tile_move_targets_the_tile_centre_and_stop_keeps_its_unit() {
    let message = Order::Move {
        unit: id(1),
        to: TileCoord::new(3, 4),
    }
    .message(9)
    .unwrap();
    assert_eq!(
        message,
        M::MoveOrder {
            sequence: 9,
            entity_id: id(1),
            destination: WorldPosition::from_tile_center(TileCoord::new(3, 4)).unwrap(),
        }
    );
    assert_eq!(
        Order::Stop { unit: id(2) }.message(1).unwrap(),
        M::StopOrder {
            sequence: 1,
            entity_id: id(2)
        }
    );
    assert!(
        Order::Move {
            unit: id(1),
            to: TileCoord::new(-1, 0)
        }
        .message(1)
        .is_none()
    );
}
