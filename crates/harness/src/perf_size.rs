//! Optimized, reproducibly compressed WASM bundle size governance.
use crate::perf::{Comparison, Verdict, compare, regression_floor};
use serde::{Deserialize, Serialize};
use std::{error::Error, fs, path::Path};

#[derive(Deserialize, Serialize)]
struct Baseline {
    version: u16,
    toolchain: String,
    wasm_bindgen: String,
    binaryen: String,
    gzip: String,
    optimized_gzip_bytes: u64,
}

fn observed_at(path: &Path) -> Result<u64, Box<dyn Error>> {
    let size = fs::metadata(path)?.len();
    if size == 0 {
        return Err("compressed WASM artifact is empty".into());
    }
    Ok(size)
}

fn proposal(size: u64) -> Baseline {
    Baseline {
        version: 1,
        toolchain: "rustc 1.99.1".into(),
        wasm_bindgen: "0.2.128".into(),
        binaryen: "108".into(),
        gzip: "1.12 -n -9".into(),
        optimized_gzip_bytes: size,
    }
}

pub fn propose() -> Result<(), Box<dyn Error>> {
    propose_at(
        Path::new("reports/perf/optimized.wasm.gz"),
        Path::new("reports/perf/wasm-proposal.json"),
    )
}

fn propose_at(input: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let proposal = proposal(observed_at(input)?);
    fs::create_dir_all(output.parent().ok_or("proposal has no parent")?)?;
    fs::write(output, serde_json::to_vec_pretty(&proposal)?)?;
    println!("review reports/perf/wasm-proposal.json; baseline was not changed");
    Ok(())
}

fn compare_observed(baseline: Baseline, size: u64) -> Result<Comparison, Box<dyn Error>> {
    if baseline.version != 1
        || baseline.toolchain != "rustc 1.99.1"
        || baseline.wasm_bindgen != "0.2.128"
        || baseline.binaryen != "108"
        || baseline.gzip != "1.12 -n -9"
    {
        return Err("WASM size baseline identity is incompatible".into());
    }
    Ok(compare(
        "optimized_gzip_wasm_bytes",
        Some(size),
        Some(baseline.optimized_gzip_bytes),
    ))
}

pub fn comparison() -> Result<Comparison, Box<dyn Error>> {
    comparison_at(
        Path::new("baselines/perf/wasm.json"),
        Path::new("reports/perf/optimized.wasm.gz"),
    )
}

pub(crate) fn comparison_at(
    baseline: &Path,
    artifact: &Path,
) -> Result<Comparison, Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read(baseline)?)?;
    compare_observed(baseline, observed_at(artifact)?)
}

/// `make perf-wasm-size`: the optimized gzip bundle against the baseline, with
/// the comparison `perf-ci` makes in CI (same threshold): it reports the trend and
/// fails only on a jump over [`crate::perf::THRESHOLD_PERCENT`].
pub fn check() -> Result<(), Box<dyn Error>> {
    check_at(Path::new("."))
}

fn check_at(root: &Path) -> Result<(), Box<dyn Error>> {
    let baseline = root.join("baselines/perf/wasm.json");
    for artifact in ["optimized.wasm.gz", "diagnostics-optimized.wasm.gz"] {
        println!("{artifact}:");
        verdict(&comparison_at(
            &baseline,
            &root.join("reports/perf").join(artifact),
        )?)?;
    }
    Ok(())
}

pub(crate) fn verdict(result: &Comparison) -> Result<(), Box<dyn Error>> {
    let (Some(observed), Some(baseline)) = (result.observed, result.baseline) else {
        return Err("optimized WASM gzip size or baseline is missing".into());
    };
    if result.verdict != Verdict::Pass {
        let cap = regression_floor(baseline) - 1;
        return Err(format!(
            "optimized WASM gzip is {observed} bytes: {:?} against baseline {baseline} (cap {cap}, {} bytes over)",
            result.verdict,
            observed.saturating_sub(cap)
        )
        .into());
    }
    let delta = observed as f64 / baseline as f64 * 100.0 - 100.0;
    println!(
        "optimized WASM gzip: {observed} bytes, baseline {baseline} ({delta:+.1}%, blocks above +{}%)",
        result.threshold_percent
    );
    Ok(())
}

#[path = "perf_size/tests.rs"]
#[cfg(test)]
mod tests;
