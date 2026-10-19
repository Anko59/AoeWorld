use super::{Entry, Result, Sample, pair};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(super) struct Data {
    pub(super) samples: Vec<Sample>,
    pub(super) unpaired: usize,
    pub(super) malformed: usize,
}

fn files(root: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            files(&entry.path(), extension, out)?;
        } else if entry.file_type()?.is_file()
            && entry.path().extension().is_some_and(|e| e == extension)
        {
            out.push(entry.path());
        }
    }
    Ok(())
}

fn paths(root: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    files(root, extension, &mut out)?;
    out.sort();
    Ok(out)
}

pub(super) fn load(root: &Path) -> Result<Data> {
    let mut data = Data::default();
    for path in paths(&root.join("timeline"), "jsonl")? {
        let mut entries = Vec::new();
        for line in fs::read_to_string(path)?.lines() {
            match serde_json::from_str::<Entry>(line) {
                Ok(entry) => entries.push(entry),
                Err(_) => data.malformed += 1,
            }
        }
        let (samples, unpaired) = pair(entries);
        data.samples.extend(samples);
        data.unpaired += unpaired;
    }
    for kind in ["evidence", "reviews"] {
        for path in paths(&root.join(kind), "json")? {
            match serde_json::from_slice::<Value>(&fs::read(path)?) {
                Ok(value) => {
                    if let Some(sample) = historical(&value, kind) {
                        if !observed(&data.samples, &sample) {
                            data.samples.push(sample.clone());
                        }
                        if kind == "evidence" {
                            for gate in value["gates"].as_array().into_iter().flatten() {
                                if let (Some(id), Some(seconds)) =
                                    (gate["gate"].as_str(), gate["seconds"].as_f64())
                                    && seconds.is_finite()
                                    && seconds >= 0.
                                {
                                    let attempts: Vec<(f64, Value)> = gate["attempts"]
                                        .as_array()
                                        .into_iter()
                                        .flatten()
                                        .filter_map(|a| {
                                            Some((a["seconds"].as_f64()?, a["verdict"].clone()))
                                        })
                                        .filter(|(s, _)| s.is_finite() && *s >= 0.)
                                        .collect();
                                    let durations = if attempts.is_empty() {
                                        vec![(seconds, gate["verdict"].clone())]
                                    } else {
                                        attempts
                                    };
                                    for (duration, verdict) in durations {
                                        let gate_sample = Sample {
                                            stage: format!("gate/{id}"),
                                            duration,
                                            detail: json!({"verdict":verdict}),
                                            ..sample.clone()
                                        };
                                        if !observed(&data.samples, &gate_sample) {
                                            data.samples.push(gate_sample);
                                        }
                                    }
                                }
                            }
                        }
                    } else if value["reused_from"].is_null() {
                        data.malformed += 1;
                    }
                }
                Err(_) => data.malformed += 1,
            }
        }
    }
    data.samples.sort_by(|a, b| a.started.total_cmp(&b.started));
    Ok(data)
}

fn historical(value: &Value, kind: &str) -> Option<Sample> {
    if !value["reused_from"].is_null() {
        return None;
    }
    let started = value["started"].as_f64()?;
    let finished = value["finished"].as_f64()?;
    if !started.is_finite() || !finished.is_finite() || started < 0. || finished < started {
        return None;
    }
    Some(Sample {
        branch: value["branch"].as_str()?.into(),
        head: value["head"].as_str()?.into(),
        stage: if kind == "reviews" {
            "review"
        } else if value["cadence"] == "recheck" {
            "recheck"
        } else {
            "gates"
        }
        .into(),
        started,
        finished,
        duration: finished - started,
        source: "history",
        detail: json!({"tier":value["tier"],"grade":value["grade"],"rounds":value["rounds"],"closing":value["closing"],"verdict":value["verdict"]}),
    })
}

fn observed(samples: &[Sample], sample: &Sample) -> bool {
    samples.iter().any(|s| {
        s.source == "journal"
            && s.branch == sample.branch
            && s.head == sample.head
            && s.stage == sample.stage
            && s.started <= sample.finished + 1.
            && s.finished + 1. >= sample.started
    })
}
