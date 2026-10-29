use crate::{BuildingKind, Order, Tech, TradeResource, UnitKind};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_protocol::{GameplayClientMessage as M, decode_gameplay_client, encode_gameplay_client};

fn id(n: u32) -> EntityId {
    EntityId(n)
}

fn all_orders() -> Vec<Order> {
    let tile = TileCoord::new(10, 20);
    vec![
        Order::SetStance {
            units: vec![id(1), id(2)],
            stance: crate::CombatStance::StandGround,
        },
        Order::Garrison {
            unit: id(2),
            building: id(7),
        },
        Order::Ungarrison {
            building: id(7),
            unit: None,
        },
        Order::Ungarrison {
            building: id(7),
            unit: Some(id(2)),
        },
        Order::Load {
            units: vec![id(1), id(2)],
            transport: id(3),
        },
        Order::Unload {
            transport: id(3),
            to: tile,
        },
        Order::SetFormation {
            units: vec![id(1), id(2)],
            formation: crate::Formation::Box,
        },
        Order::FormationMove {
            units: vec![id(1), id(2)],
            formation: crate::Formation::Line,
            to: tile,
            queued: false,
            attack_move: true,
        },
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
        Order::BuildBridge {
            builders: vec![id(3)],
            from: tile,
            to: TileCoord::new(14, 20),
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
fn bridge_order_uses_the_same_bounded_wire_as_the_browser() {
    let order = Order::BuildBridge {
        builders: vec![id(3)],
        from: TileCoord::new(10, 20),
        to: TileCoord::new(14, 20),
    }
    .message(9)
    .unwrap();
    assert_eq!(
        order,
        M::BuildBridgeOrder {
            sequence: 9,
            builder_ids: vec![id(3)],
            from: TileCoord::new(10, 20),
            to: TileCoord::new(14, 20),
            queued: false,
        }
    );
    assert_eq!(
        decode_gameplay_client(&encode_gameplay_client(&order).unwrap()).unwrap(),
        order
    );
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
    // Move, Stop, Gather, Build, BuildBridge, BuildWall, Construct, CancelConstruction, Farm,
    // Training, construction, combat, rally, queued moves, reseeds and auto-scout
    // Garrison and Ungarrison add two ordinary authenticated wire variants;
    // selective and all-unit ejection use the same current Ungarrison order.
    // Bridge, garrison/ejection, load/unload and formation share the current combined wire.
    // Combat stance adds one bounded group command.
    assert_eq!(seen.len(), 27, "distinct wire variants: {seen:?}");
}

#[test]
fn shelter_orders_keep_the_selected_unit_and_building_identifiers() {
    assert_eq!(
        Order::Garrison {
            unit: id(7),
            building: id(9)
        }
        .message(3),
        Some(M::GarrisonOrder {
            sequence: 3,
            entity_id: id(7),
            building_id: id(9)
        })
    );
    for unit in [None, Some(id(7))] {
        assert_eq!(
            Order::Ungarrison {
                building: id(9),
                unit
            }
            .message(4),
            Some(M::UngarrisonOrder {
                sequence: 4,
                building_id: id(9),
                entity_id: unit
            })
        );
    }
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

#[test]
fn formation_move_uses_one_group_command_with_queue_and_combat_flags() {
    let order = Order::FormationMove {
        units: vec![id(1), id(2)],
        formation: crate::Formation::Staggered,
        to: TileCoord::new(5, 6),
        queued: true,
        attack_move: false,
    };
    assert_eq!(
        order.message(7),
        Some(M::FormationOrder {
            sequence: 7,
            unit_ids: vec![id(1), id(2)],
            formation: crate::Formation::Staggered,
            destination: Some(WorldPosition::new(5632, 6656)),
            queued: true,
            attack_move: false
        })
    );
}

#[test]
fn vassal_rules_and_train_order_share_the_server_roster() {
    use crate::{BuildingKind, UnitKind, rules};
    assert_eq!(
        rules::trainable(BuildingKind::Castle),
        vec![UnitKind::Vassal, UnitKind::General]
    );
    let cost = rules::unit_cost(UnitKind::Vassal);
    assert_eq!(
        (cost.food, cost.wood, cost.gold, cost.stone),
        (1000, 0, 1000, 0)
    );
    assert_eq!(rules::unit_train_seconds(UnitKind::Vassal), 60);
    assert_eq!(
        Order::Train {
            building: id(9),
            kind: UnitKind::Vassal
        }
        .message(3),
        Some(M::TrainOrder {
            sequence: 3,
            building_id: id(9),
            kind: UnitKind::Vassal
        })
    );
}
