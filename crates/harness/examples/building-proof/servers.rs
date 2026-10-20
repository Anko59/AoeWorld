//! Two local, already-built comparison apps, initialized by ordinary gameplay.
use super::*;
use std::{
    io::{Read, Write},
    net::TcpStream,
    process::{Child, Command, Stdio},
    time::Instant,
};

struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn run(pack: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?;
    let output = Command::new("git")
        .args(["rev-parse", "origin/dev"])
        .output()?;
    if !output.status.success() {
        return Err("cannot resolve comparison base".into());
    }
    let revision = String::from_utf8(output.stdout)?;
    let base = root
        .join(".cache/compare/base")
        .join(revision.trim())
        .canonicalize()?;
    if !base.join("aoe-server.built").is_file() {
        return Err("run make compare once to build the fetched base".into());
    }
    let maps = root.join(".cache/tc-shots/maps").canonicalize()?;
    let map = fs::read_dir(&maps)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|s| s == "json"))
        .ok_or("prepare one map with proof mode maps")?;
    let hash = map
        .file_stem()
        .ok_or("map hash")?
        .to_string_lossy()
        .into_owned();
    if !pack
        .canonicalize()?
        .starts_with(root.join("local-assets/packs"))
    {
        return Err("pack must be local-assets/packs/<hash>".into());
    }
    let mut children = Children(Vec::new());
    for (label, tree, binary, port) in [
        ("before", base.clone(), base.join("aoe-server.built"), 18086),
        (
            "after",
            root.clone(),
            root.join("target/release/aoe-server"),
            18087,
        ),
    ] {
        let child = Command::new(binary)
            .current_dir(tree)
            .env("AOE_BIND", format!("127.0.0.1:{port}"))
            .env("AOE_SCENARIO", "smoke")
            .env("AOE_ASSET_PACK", &pack)
            .env("AOE_MAP_PACKAGE_DIRECTORY", &maps)
            .env("AOE_OPEN_MAP_PACKAGE", &hash)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(format!(
                ".cache/tc-shots/{label}-server.log"
            ))?)
            .spawn()?;
        children.0.push(child);
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let healthy = TcpStream::connect(("127.0.0.1", port)).is_ok_and(|mut stream| {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let mut prefix = [0; 12];
                stream
                    .write_all(
                        b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                    )
                    .is_ok()
                    && stream.read_exact(&mut prefix).is_ok()
                    && &prefix == b"HTTP/1.1 200"
            });
            if healthy {
                break;
            }
            if Instant::now() > deadline {
                return Err(format!("{label} startup timed out").into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        tokio::runtime::Runtime::new()?.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(180),
                village(&format!("http://127.0.0.1:{port}/"), true, false),
            )
            .await
        })??;
        println!("{label} initialized: http://127.0.0.1:{port}/");
    }
    let camera: Value = serde_json::from_slice(&fs::read(".cache/tc-shots/bookmarks.json")?)?;
    fs::write(
        ".cache/tc-shots/compare-bookmarks.json",
        serde_json::to_vec_pretty(&camera)?,
    )?;
    println!("comparison apps ready; create .cache/tc-shots/stop-proof-servers to stop");
    let deadline = Instant::now() + Duration::from_secs(30 * 60);
    while Instant::now() < deadline && !root.join(".cache/tc-shots/stop-proof-servers").exists() {
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}
