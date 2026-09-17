//! Informational Criterion samples from a shared CI host.
use aoe_scenario::SMOKE;
use serde::Serialize;
use serde_json::Value;
use std::{error::Error, fs, path::Path, process::Command};

const CASES: [&str; 5] = [
    "simulation_tick_smoke_8k",
    "viewport_query_smoke_8k",
    "snapshot_encode_1k",
    "snapshot_decode_1k",
    "palette_decode_256",
];

#[derive(Serialize)]
struct Case {
    name: &'static str,
    mean_ns: f64,
    lower_ns: f64,
    upper_ns: f64,
    raw_samples: Value,
}

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    criterion: &'static str,
    rustc: String,
    os: &'static str,
    architecture: &'static str,
    smoke_workload_hash: String,
    verdict: &'static str,
    note: &'static str,
    cases: Vec<Case>,
}

fn command(name: &str, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new(name).args(args).output()?;
    if !output.status.success() {
        return Err(format!("{name} {} failed", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn case_at(root: &Path, name: &'static str) -> Result<Case, Box<dyn Error>> {
    let base = root.join(name).join("new");
    let estimates: Value = serde_json::from_slice(&fs::read(base.join("estimates.json"))?)?;
    let mean = &estimates["mean"];
    let value = |field: &Value, key: &str| -> Result<f64, Box<dyn Error>> {
        let number = field
            .as_f64()
            .ok_or_else(|| format!("{name} missing mean {key}"))?;
        if !number.is_finite() || number <= 0.0 {
            return Err(format!("{name} has invalid mean {key}").into());
        }
        Ok(number)
    };
    let raw_samples: Value = serde_json::from_slice(&fs::read(base.join("sample.json"))?)?;
    let iterations = raw_samples["iters"]
        .as_array()
        .ok_or("Criterion iterations missing")?;
    let times = raw_samples["times"]
        .as_array()
        .ok_or("Criterion times missing")?;
    if iterations.len() < 10
        || iterations.len() != times.len()
        || iterations.iter().chain(times).any(|item| {
            item.as_f64()
                .is_none_or(|number| !number.is_finite() || number <= 0.0)
        })
    {
        return Err(format!("{name} is missing Criterion raw samples").into());
    }
    Ok(Case {
        name,
        mean_ns: value(&mean["point_estimate"], "point_estimate")?,
        lower_ns: value(&mean["confidence_interval"]["lower_bound"], "lower_bound")?,
        upper_ns: value(&mean["confidence_interval"]["upper_bound"], "upper_bound")?,
        raw_samples,
    })
}

pub fn report() -> Result<(), Box<dyn Error>> {
    report_at(Path::new("target/criterion"), Path::new("reports/perf"))?;
    println!("wrote reports/perf/timing.json and timing.md (informational)");
    Ok(())
}

fn report_at(root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let cases = CASES
        .iter()
        .map(|name| case_at(root, name))
        .collect::<Result<Vec<_>, _>>()?;
    let report = Report {
        version: 1,
        revision: command("git", &["rev-parse", "HEAD"])?,
        dirty: !command("git", &["status", "--porcelain"])?.is_empty(),
        criterion: "0.8.2",
        rustc: command("rustc", &["--version"])?,
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        smoke_workload_hash: SMOKE.workload_hash(),
        verdict: "INCONCLUSIVE",
        note: "Hosted elapsed times are informational; dedicated-hardware timing qualification is not established.",
        cases,
    };
    fs::create_dir_all(output)?;
    fs::write(
        output.join("timing.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let mut markdown = format!(
        "# Hosted Criterion timings\n\nRevision: `{}`; dirty: `{}`; verdict: `INCONCLUSIVE`.\n\n| Case | Mean (ns) |\n|---|---:|\n",
        report.revision, report.dirty
    );
    for case in &report.cases {
        markdown.push_str(&format!("| {} | {:.1} |\n", case.name, case.mean_ns));
    }
    markdown.push_str("\nHosted elapsed times are informational. Dedicated hardware qualification is not established.\n");
    fs::write(output.join("timing.md"), markdown)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_every_raw_case_and_writes_inconclusive_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("criterion");
        let output = temp.path().join("reports");
        for name in CASES {
            let directory = root.join(name).join("new");
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("estimates.json"), r#"{"mean":{"point_estimate":100.0,"confidence_interval":{"lower_bound":90.0,"upper_bound":110.0}}}"#).unwrap();
            fs::write(
                directory.join("sample.json"),
                serde_json::json!({
                    "sampling_mode": "Linear",
                    "iters": [1,2,3,4,5,6,7,8,9,10],
                    "times": [100,200,300,400,500,600,700,800,900,1000]
                })
                .to_string(),
            )
            .unwrap();
        }
        report_at(&root, &output).unwrap();
        let report: Value =
            serde_json::from_slice(&fs::read(output.join("timing.json")).unwrap()).unwrap();
        assert_eq!(report["verdict"], "INCONCLUSIVE");
        assert_eq!(report["cases"].as_array().unwrap().len(), CASES.len());
        assert_eq!(
            report["cases"][0]["raw_samples"]["times"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
        assert!(
            fs::read_to_string(output.join("timing.md"))
                .unwrap()
                .contains("Dedicated hardware qualification is not established")
        );
        fs::remove_file(root.join(CASES[0]).join("new/sample.json")).unwrap();
        assert!(report_at(&root, &output).is_err());
    }
}
