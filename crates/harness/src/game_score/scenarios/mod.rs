//! The scenarios of the game score: data, not code. A scenario says how the
//! match starts, how many ticks are played and which windows of them are
//! reported; the runner plays any of them the same way. A new scenario (the
//! battle at a choke, the march over a bridge) is one more constant below,
//! plus a `Start` when it needs a different beginning.
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

/// How a match begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Start {
    /// One village per seat, every seat from the first minute.
    Fresh,
    /// The owner's late game seeded around every village (`late_game.rs`),
    /// at this scale in percent.
    LateGame { percent: u32 },
}

/// A window of a run: ticks `from + 1` through `to`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Span {
    pub id: &'static str,
    pub from: u64,
    pub to: u64,
}

#[derive(Debug)]
pub(super) struct Scenario {
    pub id: &'static str,
    /// One line, in plain words, for the report.
    pub what: &'static str,
    pub start: Start,
    /// Ticks played, warm-up included.
    pub ticks: u64,
    /// The windows reported besides the whole run.
    pub spans: &'static [Span],
    /// Hash the world every this many ticks, to compare runs.
    pub hash_every: u64,
    /// Take the census every this many ticks.
    pub sample_every: u64,
    /// A worker count the scenario must be played at because it is not the
    /// same game at every count; `None` when it is.
    pub fixed_workers: Option<usize>,
    /// What a reader of the report should know and the table cannot say.
    pub note: &'static str,
}

/// One fresh 100-seat game, played the way a server plays it, for sixteen game
/// minutes and forty seconds. The early game is game time 1:40 to 5:00 (ticks
/// 1,000 to 3,000) and the middle game 13:20 to 16:40 (ticks 8,000 to 10,000).
pub(super) const NATURAL: Scenario = Scenario {
    id: "natural",
    what: "one fresh game, every seat played by the Rust AI",
    start: Start::Fresh,
    ticks: 10_000,
    spans: &[
        Span {
            id: "early",
            from: 1_000,
            to: 3_000,
        },
        Span {
            id: "middle",
            from: 8_000,
            to: 10_000,
        },
    ],
    hash_every: 10,
    sample_every: 50,
    fixed_workers: None,
    note: "",
};

/// The owner's late game seeded on every village (about 9,600 units at the
/// full recipe), every seat played by the Rust AI. The first game minute is
/// the warm-up: the 3,800 villagers all walk to work at once and the AI
/// brains take their first decisions. The Steward and the General start
/// working and the armies meet in the second minute, which is the window.
pub(super) const LATE: Scenario = Scenario {
    id: "late",
    what: "the late-game recipe on every village, every seat played by the Rust AI",
    start: Start::LateGame { percent: 100 },
    ticks: 1_200,
    spans: &[Span {
        id: "late",
        from: 600,
        to: 1_200,
    }],
    hash_every: 10,
    sample_every: 50,
    fixed_workers: None,
    note: "",
};

/// Every scenario, in the order of the report.
pub(super) const ALL: [&Scenario; 2] = [&NATURAL, &LATE];

pub(super) fn find(id: &str) -> Result<&'static Scenario, String> {
    ALL.iter()
        .find(|scenario| scenario.id == id)
        .copied()
        .ok_or_else(|| {
            let known: Vec<_> = ALL.iter().map(|scenario| scenario.id).collect();
            format!("game-score: no scenario {id:?}, the scenarios are {known:?}")
        })
}

/// The scenarios `make game-score` plays when none is named: all of them.
pub(super) fn pull_request_set() -> Vec<&'static Scenario> {
    ALL.to_vec()
}

impl Scenario {
    /// The windows of a run of `ticks` ticks: those of the scenario that fit,
    /// then the whole run (`whole`).
    pub fn spans_for(&self, ticks: u64) -> Vec<Span> {
        let mut spans: Vec<Span> = self
            .spans
            .iter()
            .filter(|span| span.to <= ticks)
            .copied()
            .collect();
        spans.push(Span {
            id: "whole",
            from: 0,
            to: ticks,
        });
        spans
    }

    /// The ticks a run of `ticks` ticks takes a census at: every
    /// `sample_every`, every game minute (the series), both ends of every
    /// window, the start and the end.
    pub fn sample_ticks(&self, ticks: u64, sample_every: u64) -> BTreeSet<u64> {
        let every = sample_every.max(1);
        let mut at: BTreeSet<u64> = (0..=ticks).step_by(every as usize).collect();
        at.extend((0..=ticks).step_by(super::windows::TICKS_PER_MINUTE as usize));
        for span in self.spans_for(ticks) {
            at.extend([span.from, span.to]);
        }
        at.insert(ticks);
        at
    }

    /// The scale of the late-game recipe, `over` (a flag) taking precedence.
    pub fn late_percent(&self, over: Option<u32>) -> Option<u32> {
        match self.start {
            Start::Fresh => None,
            Start::LateGame { percent } => Some(over.unwrap_or(percent)),
        }
    }
}
