use super::*;
use aoe_rendering::SceneResource;

fn amount(client: &mut Client, node: aoe_map::ResourceNode, remaining: u16, revision: u64) {
    assert!(client.resources.apply(aoe_protocol::ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision,
        changes: vec![aoe_protocol::ResourceAmount {
            id: node.id,
            remaining
        }],
    }));
}

fn present(client: &mut Client, resources: &[SceneResource]) {
    let scene = prepare(client);
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    assert!(
        crate::playground::game::map::render_cache::render_if_changed(
            client,
            &scene,
            &[],
            resources,
            camera,
            0,
            false,
        )
        .unwrap()
    );
}

#[wasm_bindgen_test]
async fn presented_picking_waits_for_resource_pixels_and_rejects_exhausted_nodes() {
    let mut client = fixture(1.0).await;
    client.units.clear();
    client.village.buildings.clear();
    client.canvas.set_width(1280);
    client.canvas.set_height(720);
    client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        tiles: vec![],
        overview: vec![],
        disabled: true,
    });
    present(&mut client, &[]);
    let berry =
        aoe_map::ResourceNode::starting(TileCoord::new(98, 98), aoe_map::ResourceKind::Food)
            .unwrap();
    amount(&mut client, berry, berry.initial_amount, 1);
    // This point is in the berry's body, outside its ground tile. A resource
    // delta must not expose a new sprite hitbox while a frame is still pending.
    let point = ScreenPoint { x: 640.0, y: 210.0 };
    let ground = crate::playground::game::map::world_at_screen(&client, point).unwrap();
    assert_ne!(
        ground.map(|v| v.floor() as i32),
        [berry.tile.x, berry.tile.y]
    );
    assert_eq!(picking::resource(&client, point), None);
    let resources = crate::playground::game::map::scene_resources(&client);
    assert_eq!(resources.len(), 1);
    present(&mut client, &resources);
    assert_eq!(
        picking::resource(&client, point),
        Some((berry.id, aoe_protocol::ResourceKind::Food))
    );
    assert!(client.resources.apply(aoe_protocol::ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 2,
        changes: vec![],
    }));
    assert_eq!(
        picking::resource(&client, point),
        None,
        "replacement snapshots can remove starting nodes"
    );
    amount(&mut client, berry, 0, 3);
    assert_eq!(
        picking::resource(&client, point),
        None,
        "live depletion takes precedence over old pixels"
    );
    // A new view has no matching presented snapshot and must use fresh bounds.
    amount(&mut client, berry, berry.initial_amount, 4);
    client.camera.center[0] += 0.5;
    let anchor = crate::playground::game::map::screen_position(&client, [98.5, 98.5]);
    let point = ScreenPoint {
        x: anchor.x,
        y: anchor.y - 40.0,
    };
    assert_eq!(
        picking::resource(&client, point),
        Some((berry.id, aoe_protocol::ResourceKind::Food))
    );
    client.rendered_frame.clear();
    assert_eq!(
        picking::resource(&client, point),
        Some((berry.id, aoe_protocol::ResourceKind::Food))
    );
    client.canvas.remove();
}
