//! The report page: sections of text, charts and tables, in the shell of
//! `shell.html` (inline style and script; nothing is loaded from the network).
use super::Data;
use super::actions::{JobReliability, Reliability};
use super::chart::render as chart;
use super::charts::{self, Family};
use super::format::{escape, grouped, label, short_date, si, signed, stamp};
use super::history::{Event, EventKind};
use super::series::{Standing, State, segments};
use crate::perf::floor_at;
use std::fmt::Write;

const SHELL: &str = include_str!("shell.html");
const LATE_GAME: &str = "simulation::late_game";
const MOVES_SHOWN: usize = 300;

pub(super) fn render(data: &Data) -> String {
    SHELL.replace("{{BODY}}", &body(data))
}

fn count(row: &str, value: u64) -> String {
    if row.starts_with("wasm_") {
        grouped(value)
    } else {
        si(value as f64)
    }
}

fn state_text(state: State) -> &'static str {
    match state {
        State::Passes => "passes",
        State::Warns => "warns",
        State::Fails => "fails",
        State::NewBaselineDue => "new baseline due",
    }
}

fn gated_phrase(rows: &[Standing]) -> String {
    let n = |state: State| rows.iter().filter(|row| row.state == state).count();
    let (fails, warns, due) = (n(State::Fails), n(State::Warns), n(State::NewBaselineDue));
    let mut parts = vec![if fails == 0 {
        format!("none of its {} gated rows fails", rows.len())
    } else {
        format!("{fails} of its {} gated rows fail", rows.len())
    }];
    if warns > 0 {
        parts.push(format!("{warns} warn"));
    }
    if due > 0 {
        parts.push(format!("{due} ask for a new baseline"));
    }
    parts.join(", ")
}

/// One sentence: what the numbers say on the latest measured `dev` commit.
pub(super) fn headline(data: &Data) -> String {
    let Some(last) = data.dev.last() else {
        return "No dev commit has been measured yet: no pull request comment names one."
            .to_owned();
    };
    let pr = last.pr.map(|pr| format!(", #{pr}")).unwrap_or_default();
    let mut clauses = vec![format!(
        "On dev at {} UTC (commit {}{pr})",
        stamp(&last.at),
        last.rev
    )];
    if !data.standing.is_empty() {
        clauses.push(gated_phrase(&data.standing));
    }
    if let Some(value) = last.rows.get(LATE_GAME) {
        let stretch = segments(&data.dev, LATE_GAME, &data.events)
            .pop()
            .unwrap_or_default();
        let redefined =
            data.events.iter().rev().find(|e| {
                e.kind == EventKind::Redefined && e.rows.iter().any(|row| row == LATE_GAME)
            });
        let change = stretch.first().map(|&first| {
            let first = &data.dev[first];
            let pct = (*value as f64 / first.rows[LATE_GAME] as f64 - 1.0) * 100.0;
            let from = match redefined {
                Some(event) => format!("{}, when it was last redefined", short_date(&event.at)),
                None => format!("{}, its first measurement", short_date(&first.at)),
            };
            format!(", {} since {from}", signed(pct))
        });
        clauses.push(format!(
            "the late game kernel counts {} instructions{}",
            si(*value as f64),
            change.unwrap_or_default()
        ));
    }
    let bundle = data.standing.iter().find(|row| row.row == "wasm_gzip");
    if let (Some(bytes), Some(gate)) = (last.rows.get("wasm_gzip"), bundle) {
        let limit = floor_at(gate.baseline, gate.limit_percent) - 1;
        let share = *bytes as f64 * 100.0 / limit as f64;
        clauses.push(format!(
            "the client bundle is {} bytes, {share:.0}% of its limit",
            grouped(*bytes)
        ));
    }
    format!("{}.", clauses.join("; ").replacen("; ", ", ", 1))
}

fn pr_link(data: &Data, pr: u64) -> String {
    format!(
        r#"<a href="https://github.com/{}/pull/{pr}">#{pr}</a>"#,
        escape(&data.repository)
    )
}

fn events_list(data: &Data, family: &Family) -> String {
    let events: Vec<&Event> = data
        .events
        .iter()
        .filter(|event| event.rows.iter().any(|row| family.rows.contains(row)))
        .collect();
    if events.is_empty() {
        return String::new();
    }
    let mut html = String::from(r#"<ul class="events">"#);
    for event in events {
        let kind = match event.kind {
            EventKind::Redefined => "redefined",
            EventKind::Accepted => "increase accepted",
        };
        let pr = event
            .pr
            .map(|pr| format!(" ({})", pr_link(data, pr)))
            .unwrap_or_default();
        let _ = write!(
            html,
            "<li><b>{}</b> {} · {kind} · {}{pr}</li>",
            escape(&event.id),
            short_date(&event.at),
            escape(&event.what)
        );
    }
    html.push_str("</ul>");
    html
}

fn section(html: &mut String, id: &str, title: &str, note: &str, inner: &str) {
    let _ = write!(html, r#"<section id="{id}"><h2>{}</h2>"#, escape(title));
    if !note.is_empty() {
        let _ = write!(html, r#"<p class="note">{}</p>"#, escape(note));
    }
    html.push_str(inner);
    html.push_str("</section>");
}

fn standing_table(data: &Data) -> String {
    if data.standing.is_empty() {
        return r#"<p class="empty">No gated row has a measurement yet.</p>"#.to_owned();
    }
    let mut html = String::from(
        r#"<div class="scroll"><table><thead><tr><th>Row</th><th class="n">On dev now</th><th class="n">Baseline</th><th class="n">Against baseline</th><th class="n">Fails above</th><th class="n">In 24 hours</th><th>Gate</th></tr></thead><tbody>"#,
    );
    for row in &data.standing {
        let limit = floor_at(row.baseline, row.limit_percent) - 1;
        let accepted = row
            .accepted_from
            .map(|from| {
                format!(
                    r#"<br><span class="quiet">increase accepted at {}</span>"#,
                    si(from as f64)
                )
            })
            .unwrap_or_default();
        let day = row.day_change.map_or("—".to_owned(), signed);
        let moved = match row.day_change {
            Some(c) if c >= 2.0 => " up",
            Some(c) if c <= -2.0 => " down",
            _ => "",
        };
        let state = state_text(row.state);
        let _ = write!(
            html,
            r#"<tr><td>{}{accepted}</td><td class="n">{}</td><td class="n">{}</td><td class="n">{}</td><td class="n quiet">{} (+{}%)</td><td class="n{moved}">{day}</td><td><span class="pill {}">{state}</span></td></tr>"#,
            escape(&label(&row.row)),
            count(&row.row, row.now),
            count(&row.row, row.baseline),
            signed(row.over_percent),
            count(&row.row, limit),
            row.limit_percent,
            state.replace(' ', "-"),
        );
    }
    html.push_str("</tbody></table></div>");
    html
}

fn moves_table(data: &Data) -> String {
    if data.moves.is_empty() {
        return r#"<p class="empty">No row has moved by 2% or more between two measured commits.</p>"#.to_owned();
    }
    let mut html = String::from(
        r#"<div class="scroll tall"><table><thead><tr><th>When (UTC)</th><th>Row</th><th class="n">Before</th><th class="n">After</th><th class="n">Change</th><th>First measured commit after it</th><th class="n">Commits since previous</th></tr></thead><tbody>"#,
    );
    for m in data.moves.iter().take(MOVES_SHOWN) {
        let (class, note) = match m.event {
            Some(EventKind::Redefined) => ("quiet", " · row redefined"),
            Some(EventKind::Accepted) => ("up", " · increase accepted"),
            None if m.change > 0.0 => ("up", ""),
            None => ("down", ""),
        };
        let pr =
            m.pr.map(|pr| format!("{} ", pr_link(data, pr)))
                .unwrap_or_default();
        let _ = write!(
            html,
            r#"<tr><td class="n">{}</td><td>{}</td><td class="n">{}</td><td class="n">{}</td><td class="n {class}">{}</td><td>{pr}{}{note}</td><td class="n quiet">{}</td></tr>"#,
            stamp(&m.at),
            escape(&label(&m.row)),
            count(&m.row, m.before),
            count(&m.row, m.after),
            signed(m.change),
            escape(&m.title),
            m.commits_since,
        );
    }
    html.push_str("</tbody></table></div>");
    if data.moves.len() > MOVES_SHOWN {
        let _ = write!(
            html,
            r#"<p class="note">The newest {MOVES_SHOWN} of {} moves; the rest are in data.json.</p>"#,
            data.moves.len()
        );
    }
    html
}

fn heat(failed: u32, runs: u32) -> (&'static str, String) {
    let share = f64::from(failed) * 100.0 / f64::from(runs.max(1));
    let class = match share {
        s if failed == 0 || s == 0.0 => "h0",
        s if s < 10.0 => "h1",
        s if s < 25.0 => "h2",
        _ => "h3",
    };
    (class, format!("{share:.0}%"))
}

fn reliability_row(job: &JobReliability, weeks: &[String]) -> String {
    let mut row = format!(
        r#"<tr><td>{}</td><td class="n">{}</td><td class="n">{}</td>"#,
        escape(&job.name),
        job.runs,
        job.failed
    );
    for week in weeks {
        match job.weeks.get(week) {
            Some(&(runs, failed)) => {
                let (class, text) = heat(failed, runs);
                let _ = write!(
                    row,
                    r#"<td class="n {class}" title="{failed} of {runs} runs">{text}</td>"#
                );
            }
            None => row.push_str(r#"<td class="n quiet">–</td>"#),
        }
    }
    let step = job.step.as_deref().map_or("—".to_owned(), escape);
    let _ = write!(
        row,
        r#"<td class="n">{}</td><td>{step}</td></tr>"#,
        job.recovered
    );
    row
}

fn reliability_section(data: &Data) -> String {
    let Some(r) = data.reliability.as_ref().filter(|r| r.runs > 0) else {
        return r#"<p class="empty">No run of the CI workflow on dev could be read from the GitHub Actions API; see the warnings above.</p>"#.to_owned();
    };
    let summary = summary(r);
    let mut html = format!(
        r#"<p class="lede">{summary}</p><div class="scroll"><table class="heat"><thead><tr><th>Job</th><th class="n">Runs</th><th class="n">Failed</th>"#
    );
    for week in &r.weeks {
        let _ = write!(html, r#"<th class="n">{}</th>"#, short_date(week));
    }
    html.push_str(r#"<th class="n">Passed on a rerun after failing</th><th>Step that failed most often</th></tr></thead><tbody>"#);
    let mut jobs: Vec<&JobReliability> = r.jobs.iter().collect();
    jobs.sort_by(|a, b| {
        (b.failed.cmp(&a.failed))
            .then(b.recovered.cmp(&a.recovered))
            .then(a.name.cmp(&b.name))
    });
    for job in jobs {
        html.push_str(&reliability_row(job, &r.weeks));
    }
    html.push_str("</tbody></table></div>");
    html
}

fn summary(r: &Reliability) -> String {
    let share = f64::from(r.runs_failed) * 100.0 / f64::from(r.runs.max(1));
    format!(
        "Of the last {} runs on dev, {} ({share:.0}%) ended with a failed job and {} failed on their first attempt and passed on a rerun.",
        r.runs, r.runs_failed, r.runs_recovered
    )
}

fn ci_section(data: &Data) -> String {
    let Some(day) = data.ci_days.last() else {
        return r#"<p class="empty">No CI run is recorded yet.</p>"#.to_owned();
    };
    format!(
        r#"<p class="lede">On {}, the longest job of a run took a median of <b>{:.1}</b> minutes over {} runs. All the jobs of a run together used <b>{:.1}</b> machine-minutes.</p>{}<h3>Machine-minutes per run</h3><p class="note">Every job's time added up: the cost of a run, not the wait for it.</p>{}"#,
        short_date(&day.day),
        day.longest,
        day.runs,
        day.total,
        chart(&charts::job_chart(data)),
        chart(&charts::machine_chart(data)),
    )
}

fn body(data: &Data) -> String {
    let mut html = String::new();
    let _ = write!(
        html,
        r#"<header><h1>AoeWorld CI report</h1><p class="lede">{}</p><p class="note">Generated {} UTC from {} versions of the CI performance comment and {} dev commits. A point is a dev commit that a pull request was compared with: {} so far. An instruction count is the number of processor instructions a fixed test world costs; it is the same on every run, so a change comes from the code.</p>"#,
        escape(&data.headline),
        stamp(&data.generated_at),
        data.inputs.comment_versions,
        data.inputs.dev_commits,
        data.inputs.measured_commits,
    );
    for warning in &data.warnings {
        let _ = write!(html, r#"<p class="note warn">{}</p>"#, escape(warning));
    }
    html.push_str("</header>");
    for family in charts::families(data).iter().filter(|f| !f.rows.is_empty()) {
        let inner = chart(&charts::instruction_chart(data, family)) + &events_list(data, family);
        section(&mut html, family.id, family.title, family.note, &inner);
    }
    section(
        &mut html,
        "bundle",
        "Client bundle: compressed size in bytes",
        "The browser client after optimisation and gzip, against its committed baseline and the limit the gate enforces.",
        &chart(&charts::bundle_chart(data)),
    );
    section(
        &mut html,
        "ci-time",
        "CI time: minutes per job, median of each day",
        "A job counts in the runs where it ran and passed; end-to-end is its slowest shard. One run per pull request comment version, taken from the job table of the CI performance comment.",
        &ci_section(data),
    );
    section(
        &mut html,
        "gate",
        "Where each gated row stands",
        "The baseline is the count committed in the repository. A row warns above +1% of it, fails above the limit shown (or +10% of its fixed reference), and asks for a new baseline when it falls more than 3% below.",
        &standing_table(data),
    );
    section(
        &mut html,
        "moves",
        "Every move of 2% or more",
        "Between two consecutive measured commits, dated at the second. When several commits merged since the previous measurement, any of them may be the cause. A move across a redefinition is not a regression.",
        &moves_table(data),
    );
    section(
        &mut html,
        "reliability",
        "Reliability: failures on pushes to dev",
        "Per job, the share of CI runs on dev that failed each week (the final attempt), and the runs where the job failed first and the run passed on a rerun. From the GitHub Actions API.",
        &reliability_section(data),
    );
    html.push_str(r#"<details><summary>How these numbers are measured, and what is left out</summary><p>On every pull request, CI counts instructions with Callgrind on fixed worlds and posts one comment that compares the pull request with the latest dev run that passed. This page keeps the dev side of those comments. A dev commit that no pull request was compared with has no point here.</p><p>Timing rows (tick times) are left out: CI prints them only when they leave a noise band, and the runners are shared. Raw comments are cached under .cache/ci-report/; <code>make ci-report</code> lists only what is new since the last run.</p><p>These kernels are generated worlds. They are a tripwire for regressions, not the speed of a real game.</p></details>"#);
    html
}
