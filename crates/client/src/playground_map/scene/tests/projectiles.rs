use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn siege_stone_scene_advances_every_native_cell_and_expires_at_impact() {
    use aoe_core::{Tick, TileRect, WorldPosition};
    let scene = PreparedScene::new(
        Camera {
            center: [10.0, 10.0],
            zoom: 1.0,
            viewport: [512.0; 2],
            focus_elevation_meters: 0.0,
        },
        Vec::new(),
    );
    let mut client = super::super::review_tests::client(&scene).await;
    client.config.tick_hz = 20;
    assert!(client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: Tick(100),
        region: TileRect::from_xywh(0, 0, 16, 16),
        disabled: false,
        tiles: vec![2; 256],
        overview: vec![],
    }));
    client.village.projectiles = vec![aoe_protocol::ProjectileState {
        id: 1,
        attacker: None,
        from: WorldPosition::new(10_000, 10_000),
        to: WorldPosition::new(12_000, 12_000),
        launched: Tick(100),
        impact: Tick(140),
        kind: ProjectileKind::Stone,
    }];
    for phase in 0..60_u16 {
        let live = 100.0 + (f64::from(phase) + 0.5) / 60.0 * 20.0;
        let tick = live.floor() as u64;
        client.village.tick = tick;
        client.presentation.reset(20);
        assert!(client.presentation.receive(tick, 0.0));
        client.last_frame = live.fract() * 50.0;
        let mut units = Vec::new();
        append(&client, &scene, &mut units);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SceneUnitKind::Stone);
        assert_eq!(u16::from(units[0].facing) >> 3, phase % 30);
        assert!(units[0].elevation_meters > 1.0);
    }
    client.village.tick = 140;
    client.presentation.reset(20);
    client.presentation.receive(140, 0.0);
    client.last_frame = 0.0;
    let mut units = Vec::new();
    append(&client, &scene, &mut units);
    assert!(units.is_empty());
    client.canvas.remove();
}

fn unpack(packed: u8) -> (u8, u8) {
    (packed & 7, packed >> 3)
}

#[wasm_bindgen_test]
fn arrows_face_the_screen_direction_of_their_ground_path() {
    // A tile step along +x then -y reads as screen east; (dx - dy, dx + dy).
    let east = flight_facing([10.0, 10.0], [14.0, 6.0]);
    assert_eq!(
        unpack(east).0,
        2,
        "mirrored west row for an eastward flight"
    );
    let west = flight_facing([10.0, 10.0], [6.0, 14.0]);
    assert_eq!(unpack(west).0, 6);
    let south = flight_facing([10.0, 10.0], [14.0, 14.0]);
    assert_eq!(unpack(south).0, 0);
    let north = flight_facing([10.0, 10.0], [6.0, 6.0]);
    assert_eq!(unpack(north).0, 4);
    // A flight along the tile axis points between the cardinals.
    let along_x = flight_facing([10.0, 10.0], [14.0, 10.0]);
    assert_eq!(unpack(along_x).0, 1, "south-east, mirrored");
    let along_y = flight_facing([10.0, 10.0], [10.0, 14.0]);
    assert_eq!(unpack(along_y).0, 7, "south-west");
}

#[wasm_bindgen_test]
fn a_zero_length_flight_is_drawn_level_facing_south() {
    let packed = flight_facing([3.0, 3.0], [3.0, 3.0]);
    assert_eq!(unpack(packed).0, 0);
}
