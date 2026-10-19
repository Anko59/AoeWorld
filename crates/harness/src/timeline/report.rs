use super::{Sample, github::Pr, history::Data};
use std::collections::{BTreeMap, BTreeSet};

fn in_window(sample: &Sample, since: f64, until: f64) -> bool {
    sample.finished >= since && sample.finished <= until
}

fn summary(samples: &[Sample]) -> BTreeMap<String, Vec<f64>> {
    let mut out = BTreeMap::<String, Vec<f64>>::new();
    for sample in samples {
        let key = match sample.detail["gate"].as_str() {
            Some(gate) => format!("{}/{gate}", sample.stage),
            None => sample.stage.clone(),
        };
        out.entry(key).or_default().push(sample.seconds());
    }
    out
}

fn stats(values: &[f64]) -> (f64, f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let median = (sorted[(n - 1) / 2] + sorted[n / 2]) / 2.;
    let p90 = sorted[(n * 9).div_ceil(10) - 1];
    (median, p90, sorted.iter().sum())
}

fn note(sample: &Sample) -> String {
    let fields: Vec<String> = [
        "gate", "tier", "grade", "round", "kind", "rounds", "closing", "ok", "verdict", "error",
    ]
    .into_iter()
    .filter_map(|key| {
        let value = &sample.detail[key];
        (!value.is_null()).then(|| format!("{key}={value}"))
    })
    .collect();
    format!(
        "{}{}",
        sample.source,
        if fields.is_empty() {
            String::new()
        } else {
            format!(", {}", fields.join(", "))
        }
    )
}

fn when(sample: &Sample) -> String {
    if sample.source == "history" && sample.stage.starts_with("gate/") {
        "(gate order)".into()
    } else {
        crate::ship::lock::utc(sample.started as u64)
    }
}

pub(super) fn render(data: &Data, prs: &[Pr], since: f64, until: f64) -> String {
    let mut samples: Vec<Sample> = data
        .samples
        .iter()
        .filter(|s| in_window(s, since, until))
        .cloned()
        .collect();
    let mut pr_lines = String::new();
    let mut assigned = BTreeSet::new();
    for pr in prs
        .iter()
        .filter(|p| p.merged_at >= since && p.merged_at <= until)
    {
        let previous = prs
            .iter()
            .filter(|p| p.head_ref_name == pr.head_ref_name && p.merged_at < pr.merged_at)
            .map(|p| p.merged_at)
            .max_by(f64::total_cmp)
            .unwrap_or(0.);
        // Join retained history first: the final push can precede the window
        // even though the derived push-to-merge interval completes inside it.
        let local: Vec<_> = data
            .samples
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.branch == pr.head_ref_name && s.finished <= pr.merged_at && s.started > previous
            })
            .collect();
        pr_lines.push_str(&format!(
            "\nPR #{} {} ({})\n",
            pr.number, pr.head_ref_name, pr.head_ref_oid
        ));
        for (i, s) in &local {
            assigned.insert(*i);
            pr_lines.push_str(&format!(
                "  {} {} {:.1}s [{}; {}]\n",
                when(s),
                s.stage,
                s.seconds(),
                s.head,
                note(s)
            ));
        }
        if let Some(first) = local.iter().map(|(_, s)| *s).find(|s| s.stage == "ship") {
            pr_lines.push_str(&format!(
                "  ship-to-merge {:.1}s (first completed ship attempt)\n",
                pr.merged_at - first.started
            ));
        } else {
            pr_lines.push_str("  ship-to-merge unavailable (no ship start)\n");
        }
        let push = local
            .iter()
            .map(|(_, s)| *s)
            .filter(|s| s.stage == "push" && s.head == pr.head_ref_oid && s.detail["ok"] != false)
            .max_by(|a, b| a.finished.total_cmp(&b.finished));
        if let Some(push) = push {
            let elapsed = pr.merged_at - push.finished;
            pr_lines.push_str(&format!("  push-to-merge {elapsed:.1}s\n"));
            samples.push(Sample {
                branch: pr.head_ref_name.clone(),
                head: pr.head_ref_oid.clone(),
                stage: "push-to-merge".into(),
                started: push.finished,
                finished: pr.merged_at,
                duration: elapsed,
                detail: serde_json::json!({}),
                source: "GitHub",
            });
        } else {
            pr_lines.push_str(&format!(
                "  created-to-merge {:.1}s (push unavailable)\n",
                (pr.merged_at - pr.created_at).max(0.)
            ));
            samples.push(Sample {
                branch: pr.head_ref_name.clone(),
                head: pr.head_ref_oid.clone(),
                stage: "created-to-merge (proxy)".into(),
                started: pr.created_at,
                finished: pr.merged_at,
                duration: (pr.merged_at - pr.created_at).max(0.),
                detail: serde_json::json!({}),
                source: "GitHub",
            });
        }
        pr_lines.push_str(&format!(
            "  GitHub created {}, merged {}\n",
            crate::ship::lock::utc(pr.created_at as u64),
            crate::ship::lock::utc(pr.merged_at as u64)
        ));
        if pr.runs.is_empty() {
            pr_lines.push_str("  CI unavailable (no returned runs for final head)\n");
        }
        for run in &pr.runs {
            if run.status == "completed"
                && let (Some(start), Some(end)) = (run.started_at, run.updated_at)
                && end >= start
            {
                pr_lines.push_str(&format!(
                    "  CI {:.1}s {} #{} {}{}\n",
                    end - start,
                    run.name,
                    run.database_id,
                    run.conclusion,
                    if end > pr.merged_at {
                        " (after merge; excluded from totals)"
                    } else {
                        ""
                    }
                ));
                if end > pr.merged_at || end < since || end > until {
                    continue;
                }
                samples.push(Sample {
                    branch: pr.head_ref_name.clone(),
                    head: pr.head_ref_oid.clone(),
                    stage: "CI".into(),
                    started: start,
                    finished: end,
                    duration: end - start,
                    detail: serde_json::json!({}),
                    source: "GitHub",
                });
            } else {
                pr_lines.push_str(&format!(
                    "  CI {} #{} {} {} (incomplete; duration unavailable)\n",
                    run.name, run.database_id, run.status, run.conclusion
                ));
            }
        }
    }
    let summaries = summary(&samples);
    let mut out = format!(
        "Delivery timeline: {} through {}\n{} local completed stages; {} unpaired boundaries; {} malformed records (all retained history)\n\nStage | count | median | p90 | total hours\n",
        crate::ship::lock::utc(since as u64),
        crate::ship::lock::utc(until as u64),
        data.samples
            .iter()
            .filter(|s| in_window(s, since, until))
            .count(),
        data.unpaired,
        data.malformed
    );
    for (stage, values) in &summaries {
        let (median, p90, total) = stats(values);
        out.push_str(&format!(
            "{stage} | {} | {median:.1}s | {p90:.1}s | {:.3}\n",
            values.len(),
            total / 3600.
        ));
    }
    out.push_str("\nTop 5 time sinks (stage totals overlap; breakdowns included):\n");
    let mut ranked: Vec<_> = summaries
        .iter()
        .filter(|(name, _)| name.as_str() != "ship")
        .map(|(name, values)| (name, values.iter().sum::<f64>()))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    for (stage, total) in ranked.into_iter().take(5) {
        out.push_str(&format!("  {stage}: {:.3} hours\n", total / 3600.));
    }
    if summaries.is_empty() {
        out.push_str("  No completed stages in window.\n");
    }
    out.push_str(&pr_lines);
    out.push_str("\nLocal stages not joined to a merged PR:\n");
    for (i, s) in data
        .samples
        .iter()
        .enumerate()
        .filter(|(_, s)| in_window(s, since, until))
    {
        if !assigned.contains(&i) {
            out.push_str(&format!(
                "  {} {} {} {:.1}s [{}; {}]\n",
                when(s),
                s.branch,
                s.stage,
                s.seconds(),
                s.head,
                note(s)
            ));
        }
    }
    out.push_str("\nLimits: historical red/lock/push waits are unavailable; evidence/reviews may have overwritten attempts.\nGate/queue/load/CI and ship totals overlap; never sum them. CI uses startedAt..updatedAt, approximately completion.\nSee docs/timeline.md for pairing, query bounds and historical coverage.\n");
    out
}
