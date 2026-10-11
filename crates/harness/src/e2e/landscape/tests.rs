use super::*;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_simulation::{GameWorld, StartSearchResult};

#[test]
fn prepared_identity_start_and_authoritative_movement_are_bounded() {
    let prepared = fields().expect("constant prepared fields");
    let package = prepared.package.clone();
    assert_eq!(
        (package.schema_version, package.generation_recipe_version),
        (
            aoe_map::MAP_SCHEMA_VERSION,
            aoe_map::GENERATION_RECIPE_VERSION
        )
    );
    assert_eq!(package.estimate.tiles_per_side, 512);
    assert!(package.source_locks.is_empty());
    assert_eq!(package.provenance.vegetation, LayerProvenance::Fallback);
    let mut world = GameWorld::from_prepared_map(
        package.clone(),
        vec![prepared.elevation],
        vec![prepared.water],
        vec![prepared.vegetation],
        Vec::new(),
    )
    .expect("production prepared GameWorld");
    let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() else {
        panic!("prepared fixture must use map terrain")
    };
    // The central chunks hold the starting glade and its connectors; probe a
    // forest chunk between the glade and the surrounding opening nodes.
    let chunk = generator.chunk(4, 4).expect("dense forest chunk");
    assert!(
        chunk
            .tiles
            .iter()
            .all(|tile| tile.biome == aoe_map::Biome::Temperate
                && tile.geographic_height_centimeters == 0
                && tile.water == aoe_map::WaterKind::None)
    );
    assert!(
        chunk
            .resources
            .iter()
            .filter(|node| node.object == aoe_map::ObjectKind::Tree)
            .count()
            > 100
    );
    let config = world.config();
    let search = world
        .terrain()
        .search_start_checked(config, 64, || false)
        .expect("bounded start search");
    let StartSearchResult::Found(start) = search else {
        panic!("fixture has no playable start: {search:?}")
    };
    // The start search also requires an exit at distance 64, not merely an
    // isolated 256-tile pocket.
    assert_eq!(world.terrain().reachable_tiles(start, config, 256), 256);
    for dy in -2..=2 {
        for dx in -2..=2 {
            assert!(
                world
                    .terrain()
                    .passable(TileCoord::new(start.x + dx, start.y + dy), config)
            );
        }
    }
    let origin = WorldPosition::from_tile_center(start).expect("start position");
    let target = WorldPosition::from_tile_center(TileCoord::new(start.x + 1, start.y))
        .expect("neighbor position");
    let unit = world.spawn_unit(PlayerId(0), origin).expect("spawn");
    assert!(world.issue_move(unit, target).expect("authoritative order"));
    for _ in 0..200 {
        world.advance();
        if world.unit(unit).expect("unit").position == target {
            break;
        }
    }
    assert_eq!(world.unit(unit).expect("unit").position, target);
    assert_eq!(
        world
            .terrain()
            .search_start_checked(config, 0, || false)
            .unwrap(),
        StartSearchResult::LimitReached
    );
    assert_eq!(
        world
            .terrain()
            .search_start_checked(config, 64, || true)
            .unwrap(),
        StartSearchResult::Cancelled
    );
    assert_eq!(
        fields().expect("repeat fixture").package.content_hash,
        package.content_hash
    );
}

#[test]
fn isolated_staging_preserves_existing_packages_without_writing_the_input() {
    let root = tempfile::tempdir().unwrap();
    let first = prepare(root.path(), None).expect("first staged fixture");
    let original = first.directory.path().join(format!("{}.json", first.hash));
    let before = fs::read(&original).unwrap();
    let extra = MapPackage::new(9, MapRequest::default(), Vec::new()).unwrap();
    let extra_path = first
        .directory
        .path()
        .join(format!("{}.json", extra.content_hash_hex()));
    publish(&extra_path, &serde_json::to_vec(&extra).unwrap()).unwrap();
    let second = prepare(root.path(), Some(first.directory.path())).expect("isolated copy");
    assert_ne!(first.directory.path(), second.directory.path());
    assert_eq!(first.hash, second.hash);
    assert_eq!(fs::read(original).unwrap(), before);
    assert_eq!(
        fs::read(
            second
                .directory
                .path()
                .join(extra_path.file_name().unwrap())
        )
        .unwrap(),
        fs::read(extra_path).unwrap()
    );
    verify_store(second.directory.path()).expect("production MapStore accepts included maps");
}

#[test]
fn production_store_rejects_missing_corrupt_and_noncanonical_pages() {
    let root = tempfile::tempdir().unwrap();
    let missing = tempfile::tempdir().unwrap();
    let package = fields().unwrap().package;
    publish(
        &missing
            .path()
            .join(format!("{}.json", package.content_hash_hex())),
        &serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    assert!(verify_store(missing.path()).is_err());
    let fixture = prepare(root.path(), None).unwrap();
    let page = fixture
        .directory
        .path()
        .join("pages")
        .join(&fixture.hash)
        .join("vegetation/0-0-0.json");
    // This is a fixed known file inside this newly created TempDir, never an
    // external source path. Restore it after each mutation.
    let before = fs::read(&page).unwrap();
    fs::write(&page, b"{}").unwrap();
    assert!(verify_store(fixture.directory.path()).is_err());
    fs::write(&page, &before).unwrap();
    let mut changed = fields().unwrap().vegetation;
    changed.potential_biome_class[0] = 2;
    fs::write(&page, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(verify_store(fixture.directory.path()).is_err());
    fs::write(&page, &before).unwrap();
    let path = fixture.directory.path().join("not-a-content-hash.json");
    fs::write(
        path,
        serde_json::to_vec(&fields().unwrap().package).unwrap(),
    )
    .unwrap();
    assert!(verify_store(fixture.directory.path()).is_err());
}

#[test]
fn production_runtime_activates_the_same_persisted_provider_twice() {
    use aoe_protocol::{
        GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage, decode_gameplay_server,
        encode_gameplay_client,
    };
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    let root = tempfile::tempdir().unwrap();
    let fixture = prepare(root.path(), None).unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = aoe_server::AppState::new(&config(fixture.directory.path()).unwrap(), "synthetic-test").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, aoe_server::app(state)).await.unwrap() });
        for _ in 0..2 {
        let (mut socket, _) = connect_async(format!("ws://{address}/game/ws")).await.unwrap();
        socket.send(Message::Binary(encode_gameplay_client(&GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION, resume_token: None,
        }).unwrap().into())).await.unwrap();
        let Message::Binary(bytes) = socket.next().await.unwrap().unwrap() else { panic!("welcome not binary") };
        let GameplayServerMessage::Welcome { resume_token: Some(token), .. } = decode_gameplay_server(&bytes).unwrap() else { panic!("controller token absent") };
        let token: String = token.0.iter().map(|byte| format!("{byte:02x}")).collect();
            let request = format!("POST /maps/{} HTTP/1.1\r\nHost: localhost\r\nX-AoeWorld-Controller-Token: {token}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", fixture.hash);
            let reply = tokio::task::spawn_blocking(move || http(address, request)).await.unwrap();
            assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
            let body: serde_json::Value = serde_json::from_str(reply.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["content_hash"], fixture.hash);
            assert_eq!(body["start_available"], true);
            assert_eq!(body["tiles_per_side"], 512);
            assert_eq!(body["source_lock_count"], 0);
            assert_eq!(body["uses_fallback_data"], true);
            let request = format!("GET /maps/{}/chunks/8/8 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n", fixture.hash);
            let reply = tokio::task::spawn_blocking(move || http(address, request)).await.unwrap();
            assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
            let body: serde_json::Value = serde_json::from_str(reply.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert!(body["payload_hex"].as_str().unwrap().starts_with("04"));
            // Activation retires the previous world and its controller session.
            let _ = socket.close(None).await;
        }
        server.abort();
        let _ = server.await;
    });
}

fn http(address: std::net::SocketAddr, request: String) -> String {
    use std::{
        io::{Read, Write},
        net::TcpStream,
        time::Duration,
    };
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

#[test]
fn selected_candidate_staging_does_not_copy_mutable_overlay_state() {
    let root = tempfile::tempdir().expect("isolated root");
    let fixture = prepare(root.path(), None).expect("synthetic source fixture");
    let mutable = fixture.directory.path().join("resource-overlays");
    fs::create_dir_all(&mutable).expect("source overlay directory");
    fs::write(mutable.join("sentinel"), b"unchanged").expect("source sentinel");
    let target = tempfile::tempdir().expect("candidate-only target");
    stage_candidate(fixture.directory.path(), target.path(), &fixture.hash)
        .expect("copy only immutable package and pages");
    assert!(
        target
            .path()
            .join(format!("{}.json", fixture.hash))
            .is_file()
    );
    assert!(target.path().join("pages").join(&fixture.hash).is_dir());
    assert!(!target.path().join("resource-overlays").exists());
    assert_eq!(
        fs::read(mutable.join("sentinel")).expect("original sentinel"),
        b"unchanged"
    );
}
