//! Optimized, reproducibly compressed WASM bundle size governance.
use crate::perf::{Comparison, compare};
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
        toolchain: "rustc 1.93.1".into(),
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
        || baseline.toolchain != "rustc 1.93.1"
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

fn comparison_at(baseline: &Path, artifact: &Path) -> Result<Comparison, Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read(baseline)?)?;
    compare_observed(baseline, observed_at(artifact)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perf::Verdict;

    #[test]
    fn size_baseline_rejects_wrong_identity_and_five_percent_regression() {
        let baseline = proposal(100);
        assert_eq!(
            compare_observed(baseline, 105)
                .expect("at threshold")
                .verdict,
            Verdict::Pass
        );
        assert_eq!(
            compare_observed(proposal(100), 106)
                .expect("over threshold")
                .verdict,
            Verdict::Regression
        );
        let mut incompatible = proposal(100);
        incompatible.binaryen = "other".to_owned();
        assert!(compare_observed(incompatible, 100).is_err());
    }

    #[test]
    fn size_artifact_must_exist_and_proposal_never_changes_baseline() {
        let temp = tempfile::tempdir().expect("directory");
        let artifact = temp.path().join("optimized.wasm.gz");
        let baseline = temp.path().join("baseline.json");
        let proposed = temp.path().join("proposal/wasm.json");
        assert!(observed_at(&artifact).is_err());
        fs::write(&artifact, []).expect("empty artifact");
        assert!(observed_at(&artifact).is_err());
        fs::write(&artifact, [7u8; 105]).expect("artifact");
        fs::write(&baseline, serde_json::to_vec(&proposal(100)).expect("JSON")).expect("baseline");
        propose_at(&artifact, &proposed).expect("proposal");
        assert_eq!(
            comparison_at(&baseline, &artifact)
                .expect("comparison")
                .verdict,
            Verdict::Pass
        );
        assert_eq!(
            serde_json::from_slice::<Baseline>(&fs::read(&proposed).expect("proposal"))
                .expect("JSON")
                .optimized_gzip_bytes,
            105
        );
        assert_eq!(
            serde_json::from_slice::<Baseline>(&fs::read(&baseline).expect("baseline"))
                .expect("JSON")
                .optimized_gzip_bytes,
            100
        );
    }
}
