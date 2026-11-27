//! The verdict lines of the section: which case it is (the same game, or a different one), then
//! determinism and crash, then the judged numbers that moved beyond their usual spread.
//! Informational: nothing here fails or blocks anything.
//!
//! When every `service` number is the merge base's, it is the same game and there is nothing to
//! judge but the cost per unit of service, which is exact (instruction counts, once a run has
//! them). When the game differs, any change of behaviour perturbs every number a little, one seed
//! is one sample, so a judged number gets a line only when it moved beyond what it does across
//! seeds on the same code (`spread`); everything else stays in the table.
use super::{
    compare::{Direction, Row, ScenarioDiff, Status, WindowDiff},
    declare::Declaration,
    numbers,
    spread::Spread,
};
use serde_json::Value;

/// A status description holds 140 characters.
const SUMMARY_LIMIT: usize = 140;

#[derive(Debug, Default, PartialEq)]
pub(super) struct Verdicts {
    pub lines: Vec<String>,
    /// Judged numbers that moved beyond their usual spread, for the worse and for the better,
    /// and how many of the worse ones a `game-score:` line of the description explains.
    pub worse: usize,
    pub explained: usize,
    pub improved: usize,
    pub determinism_holds: bool,
    pub crashed: bool,
    /// `service` numbers that differ from the merge base's, and numbers that do not.
    pub moved: usize,
    pub unchanged: usize,
    pub compared: bool,
    pub same_game: bool,
    pub summary: String,
}

pub(super) struct Facts<'a> {
    /// The branch's report, `None` when its run crashed.
    pub branch: Option<&'a Value>,
    pub crash: Option<&'a str>,
    pub diffs: &'a [ScenarioDiff],
    pub declarations: &'a [Declaration],
    pub unreadable: &'a [String],
    pub spread: &'a Spread,
}

fn workers(count: u64) -> String {
    match count {
        1 => "1 worker".to_owned(),
        n => format!("{n} workers"),
    }
}

/// One line per scenario that differs, or one line for all that agree.
fn determinism(branch: &Value) -> (Vec<String>, bool) {
    let mut agree = Vec::new();
    let mut lines = Vec::new();
    let mut holds = true;
    for scenario in branch["scenarios"].as_array().into_iter().flatten() {
        let id = scenario["id"].as_str().unwrap_or("?");
        let against = scenario["determinism"]["against"].as_array();
        let Some(first) = against.and_then(|all| all.first()) else {
            lines.push(format!("determinism: {id} not checked (one run only)"));
            continue;
        };
        let pair = format!(
            "{} and {}",
            workers(first["base_workers"].as_u64().unwrap_or(0)),
            workers(first["workers"].as_u64().unwrap_or(0))
        );
        if scenario["determinism"]["holds"] == true {
            let same_numbers = against
                .into_iter()
                .flatten()
                .all(|run| run["same_numbers"] == true);
            if same_numbers {
                agree.push(format!("{id} at {pair}"));
            } else {
                holds = false;
                lines.push(format!(
                    "determinism: {id} plays the same game at {pair} but its numbers differ"
                ));
            }
        } else {
            holds = false;
            let diagnosis = &scenario["determinism"]["diagnosis"];
            let tick = diagnosis["first_differing_tick"]
                .as_u64()
                .or_else(|| first["first_differing_tick"].as_u64());
            let what = diagnosis["hashes"]["what"]
                .as_str()
                .or_else(|| first["hashes"]["what"].as_str())
                .map(|what| format!(" ({what})"))
                .unwrap_or_default();
            lines.push(format!(
                "determinism: {id} DIFFERENT GAMES at {pair}, first differing tick {}{what}",
                tick.map_or("unknown".to_owned(), |tick| tick.to_string())
            ));
        }
    }
    if !agree.is_empty() {
        lines.insert(
            0,
            format!(
                "determinism: the same game, hash after hash and order after order: {}",
                agree.join("; ")
            ),
        );
    }
    (lines, holds)
}

fn matching<'a>(
    declarations: &'a [Declaration],
    scenario: &str,
    window: &str,
    declare: &str,
) -> Option<&'a Declaration> {
    declarations.iter().find(|declaration| {
        declaration.scenario == scenario
            && declaration.window.as_deref().is_none_or(|w| w == window)
            && declaration.number == declare
    })
}

/// The change of `row` in words: a percent, `new` from zero, `-` when a side holds no value.
pub(super) fn change(row: &Row) -> String {
    match (row.base, row.branch, row.percent()) {
        (_, _, Some(percent)) => format!("{percent:+.1}%"),
        (None, _, _) | (_, None, _) => "-".to_owned(),
        _ => "new".to_owned(),
    }
}

fn describe(scenario: &str, window: &WindowDiff, row: &Row) -> String {
    format!(
        "{scenario}/{} {}: {} to {} ({})",
        window.id,
        row.label(),
        numbers::show(&row.path, row.base),
        numbers::show(&row.path, row.branch),
        change(row)
    )
}

/// The percent to write in the declaration of `row`.
fn suggested(row: &Row) -> String {
    format!("{:+.0}%", row.percent().unwrap_or(100.0))
}

struct Tally {
    worse: usize,
    explained: usize,
    improved: Vec<String>,
    used: Vec<bool>,
    lines: Vec<String>,
    /// The line a pull request would write to explain the first unexplained move.
    example: Option<String>,
}

impl Tally {
    /// The declaration of `declare` for this window, marked used, with how it compares.
    fn explain(
        &mut self,
        declarations: &[Declaration],
        found: (&str, &str, &str),
        row: &Row,
    ) -> Option<String> {
        let (scenario, window, declare) = found;
        let declaration = matching(declarations, scenario, window, declare)?;
        if let Some(index) = declarations.iter().position(|d| d == declaration) {
            self.used[index] = true;
        }
        let measured = row.percent().unwrap_or(f64::INFINITY).abs();
        let understated = if measured > declaration.percent.abs() + 1.0 {
            format!(
                " (declared {:+.0}%, measured {})",
                declaration.percent,
                change(row)
            )
        } else {
            String::new()
        };
        Some(format!("explained{understated}: {}", declaration.reason))
    }

    /// Records one judged number that moved: beyond its usual spread (`Some` tolerance, in
    /// percent), or, in the same game, at all (`None`: exact).
    fn moved(
        &mut self,
        declarations: &[Declaration],
        scenario: &str,
        window: &WindowDiff,
        row: &Row,
        tolerance: Option<f64>,
    ) {
        let Some(declare) = numbers::find(&row.path).and_then(|known| known.declare) else {
            return;
        };
        let rule = match tolerance {
            Some(percent) => {
                format!("moved beyond its usual spread (one seed; usually within {percent:.0}%)")
            }
            None => "same game, so exact".to_owned(),
        };
        let text = describe(scenario, window, row);
        let explanation = self.explain(declarations, (scenario, &window.id, declare), row);
        match row.direction() {
            Direction::Better => self.improved.push(format!(
                "{text}{}",
                explanation.map_or(String::new(), |why| format!(" ({why})"))
            )),
            Direction::Worse => {
                self.worse += 1;
                let tail = match explanation {
                    Some(why) => {
                        self.explained += 1;
                        format!(", {why}")
                    }
                    None => {
                        self.example.get_or_insert(format!(
                            "game-score: {scenario}/{} {declare} {} because <reason>",
                            window.id,
                            suggested(row)
                        ));
                        String::new()
                    }
                };
                self.lines.push(format!("{text}: {rule}, worse{tail}"));
            }
            Direction::Same | Direction::Neither => {}
        }
    }
}

fn is_efficiency(row: &Row) -> bool {
    row.path.starts_with("efficiency.")
}

fn judge(facts: &Facts<'_>, same_game: bool, tally: &mut Tally) {
    for diff in facts
        .diffs
        .iter()
        .filter(|diff| diff.status == Status::Compared)
    {
        for window in diff.windows.iter().filter(|window| window.id != "whole") {
            for row in &window.changed {
                if same_game {
                    // The game is the same, so a cost per unit of service that grew is a cost that
                    // grew, with no chaos to hide behind: exact.
                    if is_efficiency(row) && row.direction() == Direction::Worse {
                        tally.moved(facts.declarations, &diff.id, window, row, None);
                    }
                } else if !is_efficiency(row) {
                    let tolerance = facts.spread.tolerance(&diff.id, &window.id, &row.path);
                    if tolerance.beyond(row) {
                        tally.moved(
                            facts.declarations,
                            &diff.id,
                            window,
                            row,
                            Some(tolerance.percent),
                        );
                    }
                }
            }
        }
    }
}

fn summary(v: &Verdicts, branch_only: Option<&str>) -> String {
    let text = if v.crashed {
        "informational: the game score crashed on this branch".to_owned()
    } else if !v.determinism_holds {
        "informational: runs at different worker counts played different games".to_owned()
    } else if let Some(why) = branch_only {
        format!("informational: branch numbers only ({why})")
    } else if v.same_game {
        format!(
            "informational: same game as the merge base ({} numbers unchanged)",
            v.unchanged
        )
    } else {
        let beyond = v.worse + v.improved;
        format!(
            "informational: different game: {} numbers moved, {beyond} beyond their usual spread ({} worse, {} explained)",
            v.moved, v.worse, v.explained
        )
    };
    text.chars().take(SUMMARY_LIMIT).collect()
}

pub(super) fn verdicts(facts: &Facts<'_>) -> Verdicts {
    let mut v = Verdicts {
        determinism_holds: true,
        crashed: facts.crash.is_some(),
        ..Verdicts::default()
    };
    let mut branch_only = None;
    for diff in facts.diffs {
        match &diff.status {
            Status::Compared => {
                v.compared = true;
                for window in &diff.windows {
                    v.moved += window.changed.iter().filter(|r| !is_efficiency(r)).count();
                    v.unchanged += window.unchanged;
                }
            }
            Status::NoBase(why) | Status::NotComparable(why) => {
                branch_only.get_or_insert(why.as_str());
            }
        }
    }
    v.same_game = v.compared && v.moved == 0;
    let mut tally = Tally {
        worse: 0,
        explained: 0,
        improved: Vec::new(),
        used: vec![false; facts.declarations.len()],
        lines: Vec::new(),
        example: None,
    };
    judge(facts, v.same_game, &mut tally);
    v.worse = tally.worse;
    v.explained = tally.explained;
    v.improved = tally.improved.len();
    if v.same_game {
        v.lines.push(format!(
            "**same game**: every number of every compared window is the merge base's ({} numbers)",
            v.unchanged
        ));
    } else if v.compared {
        let beyond = v.worse + v.improved;
        v.lines.push(format!(
            "**different game**: {} numbers moved, {beyond} beyond their usual spread (one seed per scenario; the spread is measured across seeds on `dev`, docs/scale/game-score.md)",
            v.moved
        ));
    }
    match facts.crash {
        Some(text) => v.lines.push(format!("crash: {text}")),
        None => v.lines.push("crash: none".to_owned()),
    }
    if let Some(branch) = facts.branch {
        let (lines, holds) = determinism(branch);
        v.lines.extend(lines);
        v.determinism_holds = holds;
    }
    for diff in facts.diffs {
        if let Status::NoBase(why) | Status::NotComparable(why) = &diff.status {
            v.lines.push(format!("{}: not compared: {why}", diff.id));
        }
    }
    v.lines.extend(tally.lines);
    if !tally.improved.is_empty() {
        let shown: Vec<&str> = tally.improved.iter().take(6).map(String::as_str).collect();
        let more = tally.improved.len().saturating_sub(shown.len());
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        v.lines.push(format!(
            "better, beyond its usual spread: {}{more}",
            shown.join("; ")
        ));
    }
    if let Some(example) = &tally.example {
        v.lines.push(format!(
            "a move that was intended is explained by a line of the pull request description, `{example}`; nothing here gates (docs/scale/game-score.md)"
        ));
    }
    for (declaration, used) in facts.declarations.iter().zip(&tally.used) {
        if numbers::by_declaration(&declaration.number).is_none() {
            v.lines.push(format!(
                "a declaration names no number the game score knows: `{}` (the names: {})",
                declaration.line(),
                numbers::declarable().join(", ")
            ));
        } else if !used {
            v.lines.push(format!(
                "a declaration matched no number that moved beyond its usual spread: `{}`",
                declaration.line()
            ));
        }
    }
    for line in facts.unreadable {
        v.lines.push(format!(
            "could not read the declaration `{line}` (the format: `game-score: <scenario>[/<window>] <number> <+N%|-N%> because <reason>`)"
        ));
    }
    v.summary = summary(&v, branch_only.filter(|_| !v.compared));
    v
}

#[cfg(test)]
#[path = "tests/explained.rs"]
mod explained;
#[cfg(test)]
#[path = "tests/verdict.rs"]
mod tests;
