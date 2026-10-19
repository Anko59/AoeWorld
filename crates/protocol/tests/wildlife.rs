use aoe_core::{EntityId, PlayerId, Tick, WorldPosition};
use aoe_protocol::*;

fn animal() -> AnimalState {
    AnimalState {
        id: EntityId(7),
        kind: AnimalKind::Sheep,
        owner: Some(PlayerId(0)),
        hp: 7,
        position: WorldPosition::new(1024, 2048),
        task: AnimalTask::Idle,
        food_remaining: 100,
        target: None,
    }
}

#[test]
fn wildlife_wire_golden_and_roundtrip() {
    let order = GameplayClientMessage::HuntOrder {
        sequence: 3,
        unit_ids: vec![EntityId(2)],
        animal_id: EntityId(7),
    };
    let bytes = encode_gameplay_client(&order).unwrap();
    assert_eq!(bytes, vec![14, 3, 1, 2, 7], "HuntOrder bytes: {bytes:?}");
    assert_eq!(decode_gameplay_client(&bytes).unwrap(), order);
    let state = GameplayServerMessage::WildlifeState {
        revision: 1,
        tick: Tick(2),
        animals: vec![animal()],
    };
    let bytes = encode_gameplay_server(&state).unwrap();
    assert_eq!(
        bytes,
        vec![8, 1, 2, 1, 7, 0, 1, 0, 7, 128, 16, 128, 32, 0, 100, 0],
        "WildlifeState bytes: {bytes:?}"
    );
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), state);
}

#[test]
fn wildlife_validates_collections_life_food_owner_and_hunt_group() {
    for ids in [
        vec![],
        vec![EntityId(1); 2],
        vec![EntityId(1); MAX_BUILD_GROUP + 1],
    ] {
        let order = GameplayClientMessage::HuntOrder {
            sequence: 1,
            unit_ids: ids,
            animal_id: EntityId(7),
        };
        assert!(encode_gameplay_client(&order).is_err());
        let raw = postcard::to_allocvec(&order).unwrap();
        assert!(decode_gameplay_client(&raw).is_err());
    }
    let a = animal();
    for invalid in [
        AnimalState { hp: 8, ..a },
        AnimalState {
            food_remaining: 101,
            ..a
        },
        AnimalState {
            task: AnimalTask::Carcass,
            ..a
        },
        AnimalState {
            kind: AnimalKind::Wolf,
            ..a
        },
        AnimalState {
            position: WorldPosition::new(-1, 0),
            ..a
        },
    ] {
        let msg = GameplayServerMessage::WildlifeState {
            revision: 1,
            tick: Tick(1),
            animals: vec![invalid],
        };
        assert!(encode_gameplay_server(&msg).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&msg).unwrap()).is_err());
    }
    for animals in [vec![a; 2], vec![a; MAX_ANIMALS + 1]] {
        let msg = GameplayServerMessage::WildlifeState {
            revision: 1,
            tick: Tick(1),
            animals,
        };
        assert!(encode_gameplay_server(&msg).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&msg).unwrap()).is_err());
    }
}

#[test]
fn hunting_tasks_are_appended_and_animal_lifecycle_is_validated() {
    assert_eq!(postcard::to_allocvec(&UnitTask::Dying).unwrap(), [8]);
    assert_eq!(postcard::to_allocvec(&UnitTask::Hunting).unwrap(), [9]);
    assert_eq!(postcard::to_allocvec(&UnitTask::Shepherding).unwrap(), [10]);
    let a = animal();
    for invalid in [
        AnimalState { hp: 0, ..a },
        AnimalState {
            target: Some(a.id),
            ..a
        },
        AnimalState {
            hp: 0,
            task: AnimalTask::Carcass,
            target: Some(EntityId(8)),
            ..a
        },
        AnimalState {
            task: AnimalTask::Attacking,
            ..a
        },
        AnimalState {
            task: AnimalTask::Fleeing,
            ..a
        },
    ] {
        let msg = GameplayServerMessage::WildlifeState {
            revision: 1,
            tick: Tick(2),
            animals: vec![invalid],
        };
        assert!(encode_gameplay_server(&msg).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&msg).unwrap()).is_err());
    }
    let msg = GameplayServerMessage::WildlifeState {
        revision: 1,
        tick: Tick(2),
        animals: vec![AnimalState {
            hp: 0,
            task: AnimalTask::Carcass,
            ..a
        }],
    };
    assert_eq!(
        decode_gameplay_server(&encode_gameplay_server(&msg).unwrap()).unwrap(),
        msg
    );
}

#[test]
fn hunting_tasks_require_villagers_and_food_loads() {
    let unit = GameplayUnitState {
        id: EntityId(1),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 1024),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Villager,
        task: UnitTask::Hunting,
        carried: None,
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
    };
    for invalid in [
        GameplayUnitState {
            kind: UnitKind::Scout,
            ..unit
        },
        GameplayUnitState {
            gathering: Some(ResourceKind::Wood),
            ..unit
        },
        GameplayUnitState {
            carried: Some(CarriedResource {
                kind: ResourceKind::Stone,
                amount: 1,
            }),
            ..unit
        },
    ] {
        let msg = GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(1),
            units: vec![invalid],
            players: vec![],
            buildings: vec![],
        };
        assert!(encode_gameplay_server(&msg).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&msg).unwrap()).is_err());
    }
}
