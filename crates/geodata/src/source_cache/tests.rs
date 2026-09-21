use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn lock() -> SourceLock {
    SourceLock {
        id: "etopo-2022-60s".to_owned(),
        provider: Provider::Noaa,
        release: "ETOPO 2022 v1".to_owned(),
        url: "https://www.ngdc.noaa.gov/example.tif".to_owned(),
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
        bytes: 3,
        native_resolution: "60 arc-seconds".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "ice surface".to_owned(),
        license_reference: "NOAA public domain".to_owned(),
    }
}

#[test]
fn source_locks_reject_unapproved_or_incomplete_sources() {
    let mut source = lock();
    assert!(source.validate().is_ok());
    source.url = "http://www.ngdc.noaa.gov/example.tif".to_owned();
    assert!(source.validate().is_err());
    source.url = "https://www.ngdc.noaa.gov/example.tif".to_owned();
    source.sha256 = "not-a-hash".to_owned();
    assert!(source.validate().is_err());
}

#[test]
fn offline_reports_only_missing_or_tampered_inputs() {
    let root = temporary_directory();
    let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
    let source = lock();
    let object = cache.object_path(&source).expect("object path");
    fs::write(&object, b"abc").expect("cached object");
    assert!(
        cache
            .offline_missing(std::slice::from_ref(&source))
            .expect("offline")
            .is_empty()
    );
    fs::write(&object, b"bad").expect("tampered object");
    assert_eq!(
        cache.offline_missing(&[source]).expect("offline"),
        vec!["etopo-2022-60s"]
    );
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn source_acquisition_hashes_sha256_and_provider_md5() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let path = root.join("source");
    fs::write(&path, b"abc").expect("source bytes");
    let (sha, sha1, md5) = file_hashes(&path).expect("digests");
    assert_eq!(
        digest_hex(&sha),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(digest_hex(&md5), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(
        digest_hex(&sha1),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    fs::remove_dir_all(root).expect("remove temporary cache");
}

fn temporary_directory() -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aoe-geodata-{}-{nanos}-{serial}",
        std::process::id()
    ))
}

struct TestServer {
    url: String,
    join: Option<JoinHandle<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            let result = join.join();
            if !thread::panicking() {
                result.expect("cache test server thread");
            }
        }
    }
}

fn serve(status: u16, headers: &[(&str, &str)], body: Vec<u8>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let address = listener.local_addr().expect("loopback address");
    let headers: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    listener
        .set_nonblocking(true)
        .expect("nonblocking loopback listener");
    let join = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    panic!("cache request deadline exceeded")
                }
                Err(error) => panic!("cache request: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("request timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("response timeout");
        consume_request(&mut stream);
        let reason = match status {
            200 => "OK",
            206 => "Partial Content",
            _ => "Error",
        };
        let mut response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        stream
            .write_all(response.as_bytes())
            .expect("response headers");
        stream.write_all(&body).expect("response body");
    });
    TestServer {
        url: format!("http://{address}"),
        join: Some(join),
    }
}

fn consume_request(stream: &mut TcpStream) {
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 512];
    while request.len() < 8 * 1024 && !request.windows(4).any(|part| part == b"\r\n\r\n") {
        let count = match stream.read(&mut buffer) {
            Ok(count) => count,
            Err(_) => break,
        };
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
    }
}

#[test]
fn cache_transport_downloads_a_bounded_payload() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let server = serve(200, &[], b"abc".to_vec());
    let partial = root.join("payload.part");
    let cancelled = AtomicBool::new(false);
    download_once(&server.url, 3, &partial, &cancelled).expect("complete download");
    assert_eq!(fs::read(&partial).expect("payload"), b"abc");
    drop(server);
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn cache_transport_resumes_only_with_a_partial_response() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let partial = root.join("payload.part");
    fs::write(&partial, b"ab").expect("partial payload");
    let server = serve(206, &[("Content-Range", "bytes 2-2/3")], b"c".to_vec());
    let cancelled = AtomicBool::new(false);
    download_once(&server.url, 3, &partial, &cancelled).expect("resumed download");
    assert_eq!(fs::read(&partial).expect("payload"), b"abc");
    drop(server);

    fs::write(&partial, b"ab").expect("partial payload");
    let server = serve(200, &[], Vec::new());
    let error = download_once(&server.url, 3, &partial, &cancelled).expect_err("ignored range");
    assert!(
        matches!(error, CacheError::Download(_)),
        "unexpected error: {error:?}"
    );
    assert!(
        !partial.exists(),
        "unexpected retained partial after {error:?}: {:?}",
        fs::metadata(&partial).ok().map(|metadata| metadata.len())
    );
    drop(server);
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn cache_transport_rejects_short_and_oversized_responses() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let partial = root.join("payload.part");
    let cancelled = AtomicBool::new(false);
    let server = serve(200, &[], b"ab".to_vec());
    let error = download_once(&server.url, 3, &partial, &cancelled).expect_err("short response");
    assert!(matches!(error, CacheError::Download(message) if message.contains("before")));
    drop(server);
    fs::remove_file(&partial).expect("remove short payload");

    let server = serve(200, &[], b"abcd".to_vec());
    let error =
        download_once(&server.url, 3, &partial, &cancelled).expect_err("oversized response");
    assert!(matches!(
        error,
        CacheError::Integrity("response exceeds expected size")
    ));
    drop(server);
    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn cache_transport_honors_cancellation_before_copying() {
    let root = temporary_directory();
    fs::create_dir_all(&root).expect("temporary root");
    let server = serve(200, &[], b"abc".to_vec());
    let cancelled = AtomicBool::new(true);
    let error = download_once(&server.url, 3, &root.join("payload.part"), &cancelled)
        .expect_err("cancelled download");
    assert!(matches!(error, CacheError::Cancelled));
    drop(server);
    fs::remove_dir_all(root).expect("remove temporary cache");
}
