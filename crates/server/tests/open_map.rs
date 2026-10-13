use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid,
    MAP_SCHEMA_VERSION, MapPackage, MapRequest, PreparedEnvironment, ProjectionMetadata,
    PyramidLevel, ordered_page_root,
};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayServerMessage, decode_gameplay_server,
    encode_gameplay_client,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn boot(directory: &std::path::Path, hash: Option<&str>) -> (Server, std::net::SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut command = Command::new(env!("CARGO_BIN_EXE_aoe-server"));
    command
        .env("AOE_BIND", address.to_string())
        .env("AOE_MAP_PACKAGE_DIRECTORY", directory)
        .env_remove("AOE_ASSET_PACK")
        .env_remove("AOE_MAP_WORKER")
        .env_remove("AOE_GEODATA_CACHE")
        .env_remove("AOE_OPEN_MAP_PACKAGE")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(hash) = hash {
        command.env("AOE_OPEN_MAP_PACKAGE", hash);
    }
    (Server(command.spawn().unwrap()), address)
}

#[tokio::test]
async fn boot_map_env_serves_metadata_to_first_session_without_open_call() {
    let directory = tempfile::tempdir().unwrap();
    let package = persist_flat_package(directory.path());
    let hash = package.content_hash_hex();
    let (mut server, address) = boot(directory.path(), Some(&hash));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut socket = loop {
        if let Ok((socket, _)) = connect_async(format!("ws://{address}/game/ws")).await {
            break socket;
        }
        if let Some(status) = server.0.try_wait().unwrap() {
            use std::io::Read;
            let mut error = String::new();
            server
                .0
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut error)
                .unwrap();
            panic!("startup failed ({status}): {error}");
        }
        assert!(Instant::now() < deadline, "startup deadline");
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    socket
        .send(Message::Binary(
            encode_gameplay_client(&GameplayClientMessage::Hello {
                version: GAMEPLAY_VERSION,
                resume_token: None,
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = message else {
        panic!("binary welcome");
    };
    let GameplayServerMessage::Welcome {
        map_content_hash,
        map_metadata,
        width_tiles,
        ..
    } = decode_gameplay_server(&bytes).unwrap()
    else {
        panic!("welcome");
    };
    assert_eq!(map_content_hash, Some(package.content_hash));
    assert_eq!(width_tiles as u64, package.estimate.tiles_per_side);
    assert!(map_metadata.is_some());
}

// Constant synthetic elevation guarantees a playable start, unlike a small
// procedural package whose start search can legitimately exhaust its budget.
fn persist_flat_package(directory: &std::path::Path) -> MapPackage {
    let pages = (0..2)
        .map(|level| {
            let side = if level == 0 { 2 } else { 1 };
            ElevationPage {
                level,
                x: 0,
                y: 0,
                width: side,
                height: side,
                geographic_height_centimeters: vec![0; usize::from(side).pow(2)],
            }
        })
        .collect::<Vec<_>>();
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        MapRequest {
            requested_side_meters: 3_840,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_920_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: pages
                    .iter()
                    .map(|page| PyramidLevel {
                        samples_per_axis: u16::from(page.width),
                        ordered_page_root: ordered_page_root(std::slice::from_ref(page)).unwrap(),
                    })
                    .collect(),
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
            hydrology_evidence: None,
        },
    )
    .unwrap();
    let hash = package.content_hash_hex();
    std::fs::write(
        directory.join(format!("{hash}.json")),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    let root = directory.join("pages").join(hash).join("elevation");
    std::fs::create_dir_all(&root).unwrap();
    for page in pages {
        std::fs::write(
            root.join(format!("{}-0-0.json", page.level)),
            serde_json::to_vec(&page).unwrap(),
        )
        .unwrap();
    }
    package
}

#[test]
fn unknown_and_invalid_boot_hash_refuse_start_with_one_line_error() {
    use std::io::Read;
    let directory = tempfile::tempdir().unwrap();
    for hash in [
        "ab".repeat(32),
        "invalid-hash".to_owned(),
        "bad\nhash".to_owned(),
        String::new(),
    ] {
        let (mut server, _) = boot(directory.path(), Some(&hash));
        let deadline = Instant::now() + Duration::from_secs(3);
        let status = loop {
            if let Some(status) = server.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "server accepted unknown hash {hash:?}"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(!status.success());
        let mut error = String::new();
        server
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error)
            .unwrap();
        assert_eq!(error.lines().count(), 1, "{error}");
        assert!(error.contains(&hash.replace('\n', "\\n")), "{error}");
        assert!(
            error.contains(&directory.path().display().to_string()),
            "{error}"
        );
        assert!(!error.starts_with("Error:"), "plain startup error: {error}");
    }
}
