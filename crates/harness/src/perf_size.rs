//! Optimized, reproducibly compressed WASM bundle size governance.
use crate::perf::{Comparison, compare};
use serde::{Deserialize, Serialize};
use std::{error::Error, fs};

#[derive(Deserialize, Serialize)]
struct Baseline {
    version: u16,
    toolchain: String,
    wasm_bindgen: String,
    binaryen: String,
    gzip: String,
    optimized_gzip_bytes: u64,
}

fn observed() -> Result<u64, Box<dyn Error>> {
    let size = fs::metadata("reports/perf/optimized.wasm.gz")?.len();
    if size == 0 {
        return Err("compressed WASM artifact is empty".into());
    }
    Ok(size)
}

pub fn propose() -> Result<(), Box<dyn Error>> {
    let proposal = Baseline {
        version: 1,
        toolchain: "rustc 1.93.1".into(),
        wasm_bindgen: "0.2.128".into(),
        binaryen: "108".into(),
        gzip: "1.12 -n -9".into(),
        optimized_gzip_bytes: observed()?,
    };
    fs::create_dir_all("reports/perf")?;
    fs::write(
        "reports/perf/wasm-proposal.json",
        serde_json::to_vec_pretty(&proposal)?,
    )?;
    println!("review reports/perf/wasm-proposal.json; baseline was not changed");
    Ok(())
}

pub fn comparison() -> Result<Comparison, Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read("baselines/perf/wasm.json")?)?;
    if baseline.version != 1
        || baseline.toolchain != "rustc 1.93.1"
        || baseline.wasm_bindgen != "0.2.128"
        || baseline.binaryen != "108"
        || baseline.gzip != "1.12 -n -9"
    {
        return Err("WASM size baseline identity is incompatible".into());
    }
    Ok(compare(
        "optimized_gzip_wasm_bytes",
        Some(observed()?),
        Some(baseline.optimized_gzip_bytes),
    ))
}
