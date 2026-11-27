//! A run watched at a choke. A line is drawn across the neck, between the centres of the two tiles
//! that block it; a unit has crossed when a step of it cuts that segment, whichever way it goes
//! (the first time, once). Going around the end of the line is not a crossing: the ends are walls.
//! Besides crossings the watch keeps how long each unit has stood still inside the neck, whether it
//! is within two tiles of the last goal it was given, and which units have fought. It only reads:
//! the world is the same with or without it. All of it is integer arithmetic on subunits.
use super::{Choke, TILE, Tile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type Point = (i64, i64);

fn point((x, y): (i32, i32)) -> Point {
    (i64::from(x), i64::from(y))
}

/// A unit within this distance of its goal has arrived (the convention of `arrival`).
const ARRIVED_WITHIN: i64 = 2 * TILE;

/// One unit as the watch sees it after a tick.
#[derive(Clone, Copy, Debug)]
pub(in crate::game_score) struct Walker {
    pub id: u32,
    /// The seat that owns it (two are told apart: the lower one and the others).
    pub seat: u16,
    pub position: (i32, i32),
    /// The unit's position changed during the tick.
    pub stepped: bool,
    /// The goal of its movement order; `None` without one.
    pub goal: Option<(i32, i32)>,
    pub alive: bool,
}

struct Mark {
    seat: u16,
    alive: bool,
    crossed: bool,
    /// Where it was the last time it was off the line, and on which side (the sign).
    last: Point,
    side: i64,
    at: Point,
    still: u64,
    goal: Option<Point>,
}

/// What one tick showed, summed over the units.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::game_score) struct Moves {
    /// Units that crossed the line in the tick.
    pub crossed: u64,
    /// The longest a unit has stood still inside the neck, as of this tick (ticks).
    pub still_peak: u64,
    /// Units alive after the tick, by seat: seat 0 first, every other seat second.
    pub alive: [u64; 2],
}

/// The census of the watch at a sampled tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct ChokeTotals {
    /// Units staged: every unit the watch has seen, alive or not, and by seat.
    pub sent: u64,
    pub sent_by_seat: [u64; 2],
    /// Units alive and still on their own side: they have not crossed the line.
    pub near_side: u64,
    /// Units alive and within two tiles of the last goal they were given.
    pub at_goal: u64,
    pub alive_by_seat: [u64; 2],
    /// Units that have struck or been struck since the start.
    pub fought: u64,
}

/// The segment across the neck, and the reach of the neck on either side of it.
#[derive(Clone, Copy)]
struct Line {
    a: Point,
    d: Point,
    /// Floor of the square root of the squared length of `d`, subunits.
    length: i64,
    /// How far from the line, along the march, a unit counts as inside the neck (subunits).
    reach: i64,
}

impl Line {
    /// Twice the signed area of the triangle `a`, `b`, `p`: its sign is the side of the line `p`
    /// is on, and its size over the length is the distance to the line.
    fn side_of(&self, p: Point) -> i64 {
        self.d.0 * (p.1 - self.a.1) - self.d.1 * (p.0 - self.a.0)
    }

    /// How far along the segment `p` projects, in subunits times the length (`0` is `a`, the
    /// length squared is `b`).
    fn along(&self, p: Point) -> i64 {
        self.d.0 * (p.0 - self.a.0) + self.d.1 * (p.1 - self.a.1)
    }

    /// `p` is between the ends and within `reach` of the line.
    fn inside(&self, p: Point) -> bool {
        (0..=self.length * self.length).contains(&self.along(p))
            && self.side_of(p).abs() <= self.reach * self.length
    }

    /// The step from `from` to `to` cuts the segment, from one side to the other.
    fn cuts(&self, from: Point, to: Point) -> bool {
        let (d1, d2) = (i128::from(self.side_of(from)), i128::from(self.side_of(to)));
        if d1.signum() * d2.signum() != -1 {
            return false;
        }
        // Where along the line the step meets it, in the same units as `along`.
        let (u, v) = (i128::from(self.along(from)), i128::from(self.along(to)));
        let meet = u + (v - u) * d1.abs() / (d1.abs() + d2.abs());
        (0..=i128::from(self.length) * i128::from(self.length)).contains(&meet)
    }
}

pub(in crate::game_score) struct ChokeWatch {
    line: Line,
    marks: BTreeMap<u32, Mark>,
    fought: BTreeSet<u32>,
}

fn seat(id: u16) -> usize {
    usize::from(id.min(1))
}

impl ChokeWatch {
    pub fn new(choke: &Choke) -> Self {
        let centre = |(x, y): Tile| {
            (
                i64::from(x) * TILE + TILE / 2,
                i64::from(y) * TILE + TILE / 2,
            )
        };
        let (a, b) = (centre(choke.ends.0), centre(choke.ends.1));
        let d = (b.0 - a.0, b.1 - a.1);
        Self {
            line: Line {
                a,
                d,
                length: (d.0 * d.0 + d.1 * d.1).isqrt().max(1),
                reach: (i64::from(choke.length_tiles) / 2 + 1) * TILE,
            },
            marks: BTreeMap::new(),
            fought: BTreeSet::new(),
        }
    }

    /// One unit after a tick; `moves` takes what it shows.
    pub fn see(&mut self, unit: Walker, moves: &mut Moves) {
        let line = self.line;
        let at = point(unit.position);
        let side = line.side_of(at).signum();
        let mark = self.marks.entry(unit.id).or_insert(Mark {
            seat: unit.seat,
            alive: true,
            crossed: false,
            last: at,
            side,
            at,
            still: 0,
            goal: None,
        });
        if !unit.alive {
            mark.alive = false;
            mark.still = 0;
            return;
        }
        moves.alive[seat(unit.seat)] += 1;
        mark.at = at;
        if let Some(goal) = unit.goal {
            mark.goal = Some(point(goal));
        }
        // A unit exactly on the line keeps the side and the place it had before: the step that
        // takes it off the line is the one that is judged.
        if side != 0 {
            if !mark.crossed && mark.side != 0 && mark.side != side && line.cuts(mark.last, at) {
                mark.crossed = true;
                moves.crossed += 1;
            }
            mark.last = at;
            mark.side = side;
        }
        mark.still = if line.inside(at) && !unit.stepped {
            mark.still + 1
        } else {
            0
        };
        moves.still_peak = moves.still_peak.max(mark.still);
    }

    /// Units that struck or were struck: `hits` are (attacker, target) pairs of one tick. Returns
    /// how many units fought for the first time.
    pub fn blows(&mut self, hits: impl IntoIterator<Item = (u32, u32)>) -> u64 {
        let mut first = 0;
        for (attacker, target) in hits {
            for id in [attacker, target] {
                if self.marks.contains_key(&id) && self.fought.insert(id) {
                    first += 1;
                }
            }
        }
        first
    }

    pub fn totals(&self) -> ChokeTotals {
        let mut totals = ChokeTotals {
            fought: self.fought.len() as u64,
            ..ChokeTotals::default()
        };
        for mark in self.marks.values() {
            totals.sent += 1;
            totals.sent_by_seat[seat(mark.seat)] += 1;
            if !mark.alive {
                continue;
            }
            totals.alive_by_seat[seat(mark.seat)] += 1;
            totals.near_side += u64::from(!mark.crossed);
            totals.at_goal += u64::from(mark.goal.is_some_and(|goal| {
                let (x, y) = (goal.0 - mark.at.0, goal.1 - mark.at.1);
                x * x + y * y <= ARRIVED_WITHIN * ARRIVED_WITHIN
            }));
        }
        totals
    }
}
