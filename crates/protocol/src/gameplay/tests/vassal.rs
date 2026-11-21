use super::*;

#[test]
fn vassal_development_order_appends_the_wire_discriminant() {
    let order = ClientMessage::VassalDevelopmentOrder {
        sequence: 1,
        vassal_id: EntityId(2),
        enabled: false,
    };
    let wire = encode_client(&order).unwrap();
    assert_eq!(wire, [35, 1, 2, 0]);
    assert_eq!(decode_client(&wire).unwrap(), order);
}

#[test]
fn vassal_report_has_current_golden_wire_and_bounded_counts() {
    let report = crate::VassalReport {
        focus: crate::VassalFocus::Fortify,
        current: crate::VassalJobs::default(),
        planned: crate::VassalJobs {
            count: 76,
            recent: [
                Some(crate::VassalJob {
                    kind: BuildingKind::WatchTower,
                    tile: TileCoord::new(20, 21),
                }),
                None,
                None,
            ],
        },
        finished: crate::VassalJobs::default(),
        military: 20,
        development: true,
        developed: false,
        alert: true,
        next: Some(BuildingKind::WatchTower),
        wait: 0,
        completed: 1,
        total: 77,
        responding: 1,
    };
    assert_eq!(
        postcard::to_allocvec(&report).unwrap(),
        [
            20, 1, 0, 1, 1, 14, 0, 1, 77, 1, 0, 0, 0, 0, 0, 76, 1, 14, 40, 42, 0, 0, 0, 0, 0, 0,
        ]
    );
    let mut b = GameplayBuildingState {
        id: EntityId(2),
        player: PlayerId(0),
        kind: BuildingKind::Castle,
        footprint: TileRect::from_xywh(10, 10, 4, 4),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 4800,
        max_hp: 4800,
        los: 11,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: Age::Castle,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: vec![],
        bridge_span: None,
    };
    b.kind = BuildingKind::Castle;
    b.garrison = vec![GarrisonUnitState {
        id: EntityId(99),
        kind: UnitKind::Vassal,
        hp: 75,
        max_hp: 75,
        steward: None,
        vassal: Some(report),
    }];
    let msg = |b| ServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        buildings: vec![b],
        players: vec![],
    };
    assert!(encode_server(&msg(b.clone())).is_ok());
    b.garrison[0].vassal.as_mut().unwrap().military = 21;
    assert!(encode_server(&msg(b.clone())).is_err());
    let report = b.garrison[0].vassal.as_mut().unwrap();
    report.military = 20;
    report.planned.count = 0;
    assert!(encode_server(&msg(b)).is_err());
}

#[test]
fn vassal_focus_order_has_the_current_golden_wire_form() {
    let order = ClientMessage::VassalFocusOrder {
        sequence: 1,
        vassal_id: EntityId(2),
        focus: crate::VassalFocus::Roads,
    };
    let wire = encode_client(&order).unwrap();
    assert_eq!(wire, [44, 1, 2, 1]);
    assert_eq!(decode_client(&wire).unwrap(), order);
    assert!(decode_client(&[44, 1, 2, 2]).is_err());
}
