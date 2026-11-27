//! The scenarios of the game score: data, not code. A scenario says how the
//! match starts, how many ticks are played and which windows of them are
//! reported; the runner plays any of them the same way. A new scenario is one
//! more constant below, plus a `Start` when it needs a different beginning.
use super::staging::Staging;
use std::collections::BTreeSet;

mod anatolia;
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
    /// No village and no AI seat: armies placed and ordered by hand at a real choke of the map
    /// (`staging`), played with the clock and nothing else.
    Staged(&'static Staging),
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

/// An army of Militia crosses the real choke of `anatolia::CHOKE`, ordered to the far side as one
/// formation: the ability to cross a choke, with nothing else going on. The window is the whole
/// run: the army has crossed and arrived long before its end (about 1,100 ticks, measured).
pub(super) const MARCH: Scenario = Scenario {
    id: "march",
    what: "an army crosses a real choke of the map (a ford), nothing else going on",
    start: Start::Staged(&anatolia::MARCH),
    ticks: 2_400,
    spans: &[Span {
        id: "crossing",
        from: 0,
        to: 2_400,
    }],
    hash_every: 10,
    sample_every: 50,
    fixed_workers: None,
    note: "",
};

/// Two mixed armies ordered across the same choke from opposite sides, attack-moving: they meet in
/// the neck. The window is the whole run, seven and a half game minutes.
pub(super) const BATTLE: Scenario = Scenario {
    id: "battle",
    what: "two armies meet in a real choke of the map, nothing else going on",
    start: Start::Staged(&anatolia::BATTLE),
    ticks: 4_500,
    spans: &[Span {
        id: "fight",
        from: 0,
        to: 4_500,
    }],
    hash_every: 10,
    sample_every: 50,
    fixed_workers: None,
    note: "",
};

/// Every scenario, in the order of the report.
pub(super) const ALL: [&Scenario; 4] = [&NATURAL, &LATE, &MARCH, &BATTLE];

pub(super) fn find(id: &str) -> Result<&'static Scenario, String> {
    ALL.iter()
        .find(|scenario| scenario.id == id)
        .copied()
        .ok_or_else(|| {
            let known: Vec<_> = ALL.iter().map(|scenario| scenario.id).collect();
            format!("game-score: no scenario {id:?}, the scenarios are {known:?}")
        })
}

/// The scenarios `make game-score` plays when none is named, on the map `map_hash`: every one that
/// is a scenario of that map (the staged ones are scenarios of one map each).
pub(super) fn pull_request_set(map_hash: &str) -> Vec<&'static Scenario> {
    ALL.iter()
        .copied()
        .filter(|scenario| scenario.applies_to(map_hash).is_ok())
        .collect()
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
            Start::Fresh | Start::Staged(_) => None,
            Start::LateGame { percent } => Some(over.unwrap_or(percent)),
        }
    }

    /// The map a staged scenario is staged on, by content hash; `None` for a match any map can host.
    #[cfg(test)]
    pub fn map(&self) -> Option<&'static str> {
        match self.start {
            Start::Staged(staging) => Some(staging.map),
            Start::Fresh | Start::LateGame { .. } => None,
        }
    }

    /// Whether the scenario can be played on the map `map_hash`: the matches played by the AI can
    /// be played on any, a staged one only on the map its choke was surveyed on.
    pub fn applies_to(&self, map_hash: &str) -> Result<(), String> {
        match self.start {
            Start::Staged(staging) if staging.map != map_hash => Err(format!(
                "game-score: the scenario {} is staged at a choke of map {}, not of {}",
                self.id,
                &staging.map[..8.min(staging.map.len())],
                &map_hash[..8.min(map_hash.len())]
            )),
            _ => Ok(()),
        }
    }

    /// Seats in the match: `requested` (a flag) for a match played by the AI, the armies' own for
    /// a staged scenario.
    pub fn seats(&self, requested: usize) -> usize {
        match self.start {
            Start::Staged(staging) => staging.seats(),
            Start::Fresh | Start::LateGame { .. } => requested,
        }
    }

    /// Whether the scenario has the number at `path` (of `service`, as the report names it): the
    /// choke numbers only a staged scenario has, the fight numbers only one with two armies.
    #[cfg(test)]
    pub fn measures(&self, path: &str) -> bool {
        match (path.split('.').next(), self.start) {
            (Some("choke"), Start::Staged(_)) => true,
            (Some("battle"), Start::Staged(staging)) => staging.seats() > 1,
            (Some("choke" | "battle"), _) => false,
            _ => true,
        }
    }
}
