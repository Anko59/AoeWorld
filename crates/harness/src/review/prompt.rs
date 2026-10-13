//! Reviewer prompts. Every reviewer gets the same frozen subject (task, diff,
//! deterministic facts) plus its persona; later rounds show the other
//! reviewers' findings, anonymized and in a shuffled order, for a verdict on
//! each. The answer always goes between the AOE-REVIEW markers as JSON.
use super::protocol::{BEGIN, END, Finding, Status};
use std::path::Path;

/// Diff text beyond this is cut; reviewers read the files themselves.
const DIFF_LIMIT: usize = 90_000;
/// Every runner passes the prompt as one argument, and Linux refuses a single
/// argument over 128 KiB (MAX_ARG_STRLEN): stay well below it.
pub(crate) const PROMPT_LIMIT: usize = 120_000;
/// Each reported field shown to other reviewers is cut to this many bytes...
const FIELD_LIMIT: usize = 1_500;
/// ...and all findings shown in one prompt share this budget.
const LISTING_LIMIT: usize = 40_000;

/// The per-field budget when `count` fields share the listing.
pub(super) fn field_limit(count: usize) -> usize {
    FIELD_LIMIT.min(LISTING_LIMIT / count.max(1))
}

pub(crate) struct Subject {
    pub(crate) head: String,
    pub(crate) merge_base: String,
    pub(crate) task: String,
    pub(crate) stat: String,
    pub(crate) diff: String,
    pub(crate) facts: String,
    /// Non-empty for an increment: what is judged and what is not.
    pub(crate) scope: String,
}

/// The scope of a review of the commits added after an approved one.
pub(super) fn scope_note(increment: &super::Increment, files: &[String], from: &str) -> String {
    let shown: Vec<&str> = files.iter().take(40).map(String::as_str).collect();
    format!(
        "This branch was already reviewed and approved at `{}` ({}/10, {} tier). You review ONLY what came after it: a {}. Judge the diff below and nothing else. The approved code is read-only context: do not re-open it. A finding must cite a line of the diff below; a finding anywhere else is recorded as a note and can never block. Read the diff yourself with `git diff {} HEAD -- {}`.",
        &increment.approved[..12.min(increment.approved.len())],
        increment.grade,
        increment.tier,
        increment.sentence(),
        &from[..12.min(from.len())],
        shown.join(" ")
    )
}

/// Personas, preamble and grader come from origin/dev, like the config.
pub(super) fn read(root: &Path, relative: &str) -> Result<String, String> {
    super::trusted(root, relative)
}

pub(super) fn cut(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Build a prompt around the subject, shrinking the diff until it fits.
pub(super) fn fit(subject: &Subject, build: impl Fn(&str) -> String) -> Result<String, String> {
    let mut limit = DIFF_LIMIT;
    loop {
        let prompt = build(&subject_text(subject, limit));
        if prompt.len() <= PROMPT_LIMIT {
            return Ok(prompt);
        }
        if limit == 0 {
            return Err(format!(
                "the review prompt is {} bytes even without the diff (limit {PROMPT_LIMIT})",
                prompt.len()
            ));
        }
        limit = limit.saturating_sub((prompt.len() - PROMPT_LIMIT).max(4_096));
    }
}

/// The task, file list and facts get a fixed budget each; the diff gets the rest.
const PART_LIMIT: usize = 20_000;

fn subject_text(subject: &Subject, diff_limit: usize) -> String {
    let part = |text: &str| {
        let text = text.trim();
        if text.len() > PART_LIMIT {
            format!("{}\n[cut here]", cut(text, PART_LIMIT))
        } else {
            text.to_owned()
        }
    };
    let mut diff = subject.diff.clone();
    if diff.len() > diff_limit {
        let mut cut = diff_limit;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n[diff cut here: read the remaining files with `git diff` yourself]\n");
    }
    let intro = if subject.scope.is_empty() {
        format!(
            "`git diff {}..{}` (run it yourself to read more):",
            &subject.merge_base[..12.min(subject.merge_base.len())],
            &subject.head[..12.min(subject.head.len())]
        )
    } else {
        part(&subject.scope)
    };
    format!(
        "## The task\n\n{}\n\n## The change\n\n{intro}\n\n```\n{}\n```\n\n```diff\n{diff}\n```\n\n## Deterministic facts (not findings)\n\n{}\n",
        part(&subject.task),
        part(&subject.stat),
        part(&subject.facts)
    )
}

/// The answer shapes shown to reviewers are deliberately not valid JSON
/// (`<…>` placeholders), so an echoed prompt can never pass as an answer.
const FINDINGS_SCHEMA: &str = r#"{"findings": [{"file": "<path>", "line": <number>, "severity": "<critical|major|minor|nit>", "category": "<correctness|spec|test_integrity|safety|performance|maintainability|scope>", "claim": "<one sentence>", "trigger": "<concrete input or sequence>", "expected_vs_actual": "<what should happen vs what does>", "evidence": "<file:line citations or quoted requirement>"}, <…>]}"#;

pub(crate) fn first_round(root: &Path, persona: &str, subject: &Subject) -> Result<String, String> {
    let preamble = read(root, "gates/review/preamble.md")?;
    let angle = read(root, &format!("gates/review/personas/{persona}.md"))?;
    fit(subject, |text| {
        format!(
            "{preamble}\n\n## Your angle\n\n{angle}\n\n{text}\n## Your answer\n\nInvestigate, then end your reply with exactly one JSON object between the markers, nothing else after it:\n\n{BEGIN}\n{FINDINGS_SCHEMA}\n{END}\n\nAn empty list (`{{\"findings\": []}}`) is a good answer.\n"
        )
    })
}

/// A deterministic shuffle of the findings this reviewer did not report.
fn others(findings: &[Finding], reviewer: usize, round: u32) -> Vec<&Finding> {
    let mut list: Vec<&Finding> = findings.iter().filter(|f| f.reporter != reviewer).collect();
    let key = |f: &&Finding| blake3::hash(format!("{}:{reviewer}:{round}", f.id).as_bytes());
    list.sort_by_key(|f| *key(f).as_bytes());
    list
}

pub(crate) fn has_work(findings: &[Finding], reviewer: usize) -> bool {
    findings.iter().any(|f| f.reporter != reviewer)
}

/// Which cross-examination round: its number, whether it is the last one
/// (no new findings) and whether it belongs to a closing review.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Round {
    pub(crate) number: u32,
    pub(crate) last: bool,
    pub(crate) closing: bool,
}

pub(crate) fn cross_round(
    root: &Path,
    persona: &str,
    subject: &Subject,
    findings: &[Finding],
    reviewer: usize,
    at: Round,
) -> Result<String, String> {
    let Round {
        number: round,
        last,
        closing,
    } = at;
    let mut listing = String::new();
    let shown = others(findings, reviewer, round);
    let limit = field_limit(shown.len() * 4);
    for finding in shown {
        let reported = &finding.reported;
        let field = |text: &str| cut(text, limit).to_owned();
        listing.push_str(&format!(
            "### {} ({:?})\n- where: {}{}\n- claim: {}\n- trigger: {}\n- expected vs actual: {}\n- evidence: {}\n\n",
            finding.id,
            reported.severity,
            field(&reported.file),
            reported.line.map(|l| format!(":{l}")).unwrap_or_default(),
            field(&reported.claim),
            field(&reported.trigger),
            field(&reported.expected_vs_actual),
            field(&reported.evidence)
        ));
    }
    let new = if last {
        "This is the last round: new findings are no longer accepted."
    } else {
        "You may add a new finding only if it is `major` or `critical`."
    };
    let task = if closing && !subject.scope.is_empty() {
        "Closing review of the commits added after an approved review: audit the diff above and nothing else; the approved code is read-only context. In round 1 there may be no finding to vote on: report a new finding only if it is inside the diff above. Later, vote on the other reviewers' findings: `upheld` if the problem is real and inside the diff, `refuted` (cite the code) if not, `partial` or `unverifiable`."
    } else if closing {
        "Closing review: verify the other reviewers' findings are fixed.\n\nEarlier reviews of this branch confirmed the findings below; the diff above is what changed since (the fixes). For each, vote `upheld` if the problem is still present at HEAD, `partial` if it is only partly fixed, `refuted` if it is fixed (cite the fixing code and the test that proves it) or `unverifiable`. A new finding must be in the diff above."
    } else {
        "verify the other reviewers' findings\n\nAssume each finding below is wrong until the code proves it right. Do not agree for its own sake and do not refute for its own sake. Vote on every finding below: `upheld`, `partial` (real but a different severity or scope), `refuted` (cite the code that disproves it) or `unverifiable`."
    };
    let preamble = read(root, "gates/review/preamble.md")?;
    let angle = read(root, &format!("gates/review/personas/{persona}.md"))?;
    fit(subject, |text| {
        format!(
            "{preamble}\n\n## Your angle\n\n{angle}\n\n{text}\n## Round {round}: {task} {new}\n\n{listing}## Your answer\n\nEnd your reply with exactly one JSON object between the markers:\n\n{BEGIN}\n{{\"verdicts\": [{{\"id\": \"<finding id>\", \"verdict\": \"<upheld|partial|refuted|unverifiable>\", \"evidence\": \"<file:line and why>\"}}, <one per finding>], \"findings\": [<new major or critical findings, if allowed>]}}\n{END}\n"
        )
    })
}

pub(crate) fn grading(
    root: &Path,
    subject: &Subject,
    findings: &[Finding],
) -> Result<String, String> {
    let mut listing = String::new();
    for finding in findings {
        let status = match finding.status {
            Status::Confirmed => "CONFIRMED",
            Status::Disputed => "disputed",
            Status::Refuted => "refuted",
        };
        listing.push_str(&format!(
            "- {} [{status}, {:?}] {} — {}\n",
            finding.id,
            finding.reported.severity,
            cut(&finding.reported.file, 300),
            cut(&finding.reported.claim, field_limit(findings.len()))
        ));
    }
    if listing.is_empty() {
        listing = "- no findings\n".into();
    }
    let grader = read(root, "gates/review/grader.md")?;
    fit(subject, |text| {
        format!(
            "{grader}\n\n{text}\n## Findings after review\n\n{listing}\n## Your answer\n\nEnd your reply with exactly one JSON object between the markers:\n\n{BEGIN}\n{{\"grade\": <1-10>, \"summary\": \"<at most two plain lines>\"}}\n{END}\n"
        )
    })
}
