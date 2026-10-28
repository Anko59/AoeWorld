//! Append-only Feudal roster and technology indexes.
use aoe_core::EntityId;
use aoe_protocol::*;

#[test]
fn military_training_and_research_have_pinned_wire_indexes() {
    for (kind, index) in [
        (UnitKind::Scout, 1),
        (UnitKind::Archer, 4),
        (UnitKind::Skirmisher, 5),
        (UnitKind::Spearman, 6),
        (UnitKind::ManAtArms, 7),
    ] {
        let order = GameplayClientMessage::TrainOrder {
            sequence: 1,
            building_id: EntityId(2),
            kind,
        };
        let bytes = encode_gameplay_client(&order).unwrap();
        assert_eq!(bytes, vec![5, 1, 2, index]);
        assert_eq!(decode_gameplay_client(&bytes).unwrap(), order);
    }
    for (tech, index) in [
        (Tech::ManAtArms, 8),
        (Tech::Forging, 9),
        (Tech::ScaleMailArmor, 10),
        (Tech::ScaleBardingArmor, 11),
        (Tech::Fletching, 12),
        (Tech::PaddedArcherArmor, 13),
    ] {
        let order = GameplayClientMessage::ResearchOrder {
            sequence: 1,
            building_id: EntityId(2),
            tech,
        };
        let bytes = encode_gameplay_client(&order).unwrap();
        assert_eq!(bytes, vec![13, 1, 2, index]);
        assert_eq!(decode_gameplay_client(&bytes).unwrap(), order);
    }
}

#[test]
fn projectile_wire_is_bounded_ordered_and_pins_the_appended_variant() {
    use aoe_core::{Tick, WorldPosition};
    let p = ProjectileState {
        id: 7,
        attacker: Some(EntityId(3)),
        from: WorldPosition::new(1024, 2048),
        to: WorldPosition::new(4096, 2048),
        launched: Tick(10),
        impact: Tick(20),
        kind: ProjectileKind::Arrow,
    };
    let message = GameplayServerMessage::Projectiles {
        revision: 1,
        tick: Tick(11),
        projectiles: vec![p],
    };
    let bytes = encode_gameplay_server(&message).unwrap();
    let golden = include_str!("../../../browser/tests/wire/projectiles.hex")
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bytes, golden);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
    for projectiles in [
        vec![p; MAX_PROJECTILES + 1],
        vec![p, p],
        vec![ProjectileState {
            impact: Tick(10),
            ..p
        }],
        vec![ProjectileState {
            from: WorldPosition::new(-1, 0),
            ..p
        }],
    ] {
        let message = GameplayServerMessage::Projectiles {
            revision: 1,
            tick: Tick(11),
            projectiles,
        };
        assert!(encode_gameplay_server(&message).is_err());
    }
}
