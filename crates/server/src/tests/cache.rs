use crate::{AppState, Config, app};
use aoe_scenario::SMOKE;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn responses_require_revalidation_including_assets_and_errors() {
    let pack = tempfile::tempdir().unwrap();
    std::fs::write(pack.path().join("manifest.json"), b"{}").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = AppState::new(
        &Config {
            bind: address,
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: Some(pack.path().to_owned()),
            map_package_directory: None,
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "cache-test",
    )
    .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
    for (method, path, status) in [
        ("GET", "/health", 200),
        ("GET", "/asset-pack/manifest.json", 200),
        ("HEAD", "/asset-pack/manifest.json", 200),
        ("GET", "/missing-cache-test-file", 404),
        ("POST", "/health", 405),
    ] {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream
            .write_all(
                format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let response = String::from_utf8_lossy(&bytes);
        let headers = response.split("\r\n\r\n").next().unwrap();
        assert_eq!(
            headers.split_whitespace().nth(1).unwrap(),
            status.to_string(),
            "{method} {path}: {headers}"
        );
        assert!(
            headers
                .lines()
                .any(|line| line.eq_ignore_ascii_case("cache-control: no-cache")),
            "{method} {path}: {headers}"
        );
    }
    server.abort();
    let _ = server.await;
}
