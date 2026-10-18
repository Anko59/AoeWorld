//! Per-model aggregation of the ledger and its plain-text table.
use super::record::Pr;
use std::collections::{BTreeMap, BTreeSet};

/// The grade at which a review lets a pull request merge.
const UNATTRIBUTED: &str = "(unattributed)";

#[derive(Debug, Default, PartialEq)]
pub(super) struct Row {
    pub(super) prs: u32,
    pub(super) reviewed: u32,
    /// First-review grades known / their sum.
    pub(super) graded: u32,
    pub(super) grade_sum: u32,
    /// Reviewed pull requests whose first review passed.
    pub(super) first_pass: u32,
    pub(super) counted: u32,
    pub(super) confirmed_sum: u32,
    pub(super) closing: u32,
    pub(super) split: u32,
    pub(super) reverted: u32,
}

/// Whether the first review of a pull request passed: a full review in a merged
/// description passed first time; a closing review did only when it followed an
/// approval (it names the approved grade), not when it followed a failed review.
fn first_pass(pr: &Pr) -> Option<bool> {
    let review = pr.review.as_ref()?;
    Some(!review.closing || review.first_grade.is_some())
}

fn add(row: &mut Row, pr: &Pr, reverted: &BTreeSet<u64>) {
    row.prs += 1;
    row.split += u32::from(pr.split);
    row.reverted += u32::from(reverted.contains(&pr.number));
    let Some(review) = &pr.review else { return };
    row.reviewed += 1;
    if let Some(grade) = review.first_grade {
        row.graded += 1;
        row.grade_sum += u32::from(grade);
    }
    row.first_pass += u32::from(first_pass(pr) == Some(true));
    if let Some(confirmed) = review.confirmed {
        row.counted += 1;
        row.confirmed_sum += confirmed;
    }
    row.closing += u32::from(review.closing);
}

/// Rows per implementer model. A pull request counts for every model that
/// worked on it, so the rows add up to more than the number of pull requests.
pub(super) fn by_implementer(prs: &[&Pr], reverted: &BTreeSet<u64>) -> BTreeMap<String, Row> {
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for pr in prs {
        let none = [UNATTRIBUTED.to_owned()];
        let models = if pr.models.is_empty() {
            &none[..]
        } else {
            &pr.models[..]
        };
        for model in models {
            add(rows.entry(model.clone()).or_default(), pr, reverted);
        }
    }
    rows
}

/// Rows per reviewer model: how the pull requests it reviewed fared.
pub(super) fn by_reviewer(prs: &[&Pr], reverted: &BTreeSet<u64>) -> BTreeMap<String, Row> {
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for pr in prs {
        if let Some(review) = &pr.review {
            add(rows.entry(review.model.clone()).or_default(), pr, reverted);
        }
    }
    rows
}

fn mean(sum: u32, n: u32, places: usize) -> String {
    if n == 0 {
        "unknown".to_owned()
    } else {
        format!("{:.places$}", f64::from(sum) / f64::from(n))
    }
}

fn share(part: u32, whole: u32) -> String {
    if whole == 0 {
        "unknown".to_owned()
    } else {
        format!("{:.0}%", 100.0 * f64::from(part) / f64::from(whole))
    }
}

pub(super) fn render(title: &str, label: &str, rows: &BTreeMap<String, Row>) -> String {
    let header = [
        "PRs",
        "reviewed",
        "first grade",
        "first pass",
        "confirmed",
        "closing",
        "split",
        "reverted",
    ];
    let mut cells: Vec<(String, Vec<String>)> = rows
        .iter()
        .map(|(model, r)| {
            let values = vec![
                r.prs.to_string(),
                r.reviewed.to_string(),
                mean(r.grade_sum, r.graded, 1),
                share(r.first_pass, r.reviewed),
                mean(r.confirmed_sum, r.counted, 1),
                r.closing.to_string(),
                r.split.to_string(),
                r.reverted.to_string(),
            ];
            (model.clone(), values)
        })
        .collect();
    cells.sort_by(|a, b| {
        b.1[0]
            .parse::<u32>()
            .unwrap_or(0)
            .cmp(&a.1[0].parse::<u32>().unwrap_or(0))
            .then(a.0.cmp(&b.0))
    });
    let width = cells
        .iter()
        .map(|(m, _)| m.len())
        .chain([label.len()])
        .max()
        .unwrap_or(0);
    let mut out = format!("{title}\n{label:<width$}");
    for h in header {
        out.push_str(&format!("  {h:>11}"));
    }
    out.push('\n');
    for (model, values) in &cells {
        out.push_str(&format!("{model:<width$}"));
        for v in values {
            out.push_str(&format!("  {v:>11}"));
        }
        out.push('\n');
    }
    if cells.is_empty() {
        out.push_str("(no pull requests)\n");
    }
    out
}
