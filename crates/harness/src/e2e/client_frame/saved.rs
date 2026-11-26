//! Saved-game inputs and provenance for the real-time browser driver.
use super::{Options, Result, run_browser};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

// The benchmark server binds only a disposable loopback port. This token
// enables its existing fog control and read-only population queries.
pub(super) const LOCAL_ADMIN_TOKEN: &str = "local-client-frame-bench";

pub(super) fn run(
    options: &Options,
    root: &Path,
    client_package: &Path,
    url: &str,
    side: u64,
    save_id: &str,
) -> Result<()> {
    if save_id.is_empty()
        || !save_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid save ID".into());
    }
    let saves = options.package_directory.join("saves");
    let card: Value = serde_json::from_slice(&fs::read(saves.join(format!("{save_id}.json")))?)?;
    if card["id"] != save_id || card["map_hash"] != options.content_hash {
        return Err("save card does not match the requested save and map".into());
    }
    let hash_file =
        |path| -> Result<String> { Ok(blake3::hash(&fs::read(path)?).to_hex().to_string()) };
    let config = options.out.with_extension("input.json");
    let git = |args: &[&str]| -> Result<Vec<u8>> {
        let output = Command::new("git").args(args).output()?;
        if !output.status.success() {
            return Err("cannot identify benchmark source".into());
        }
        Ok(output.stdout)
    };
    let client_path = client_package.to_str().ok_or("non-UTF8 client path")?;
    fs::write(
        &config,
        serde_json::to_vec(&json!({
            "url": url,
            "side": side,
            "package": options.content_hash,
            "save": card,
            "save_blake3": hash_file(saves.join(format!("{save_id}.save")))?,
            "asset_manifest_blake3": hash_file(options.asset_pack.join("manifest.json"))?,
            "wasm_blake3": hash_file(client_package.join("aoe_client_bg.wasm"))?,
            "scene_seconds": options.scene_seconds,
            "client_package_directory": client_package,
            "admin_token": LOCAL_ADMIN_TOKEN,
            "revision": String::from_utf8(git(&["rev-parse", "HEAD"])?)?.trim(),
            "dirty": !git(&["status", "--porcelain"])?.is_empty(),
        "client_revision": String::from_utf8(git(&["-C", client_path, "rev-parse", "HEAD"])?)?.trim(),
        "client_source_diff_blake3": blake3::hash(&git(&["-C", client_path, "diff", "HEAD", "--binary"])?).to_hex().to_string(),
            "source_diff_blake3": blake3::hash(&git(&["diff", "HEAD", "--binary"])?).to_hex().to_string(),
        }))?,
    )?;
    run_browser(root, "stutter.mjs", &config, &options.out, 0)?;
    let report: Value = serde_json::from_slice(&fs::read(&options.out)?)?;
    for scene in ["fast_pan", "crowded_hold"] {
        let row = &report["scenes"][scene]["summary"];
        println!(
            "{scene}: p50 {} ms, p99 {} ms, max {} ms, >50 ms {}/min, >100 ms {}/min",
            row["p50_ms"],
            row["p99_ms"],
            row["max_ms"],
            row["over_50"]["per_minute"],
            row["over_100"]["per_minute"],
        );
    }
    println!("Saved-game frame report: {}", options.out.display());
    Ok(())
}
