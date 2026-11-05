//! The scoreboard's cells: every pair with every army, with and without a General.
use super::run::{Army, Pair, march, setup};
use super::*;

/// The owner's first real-map run: its bridge is on a tributary that leads
/// nowhere near the goal, so the true walk is 1,084 tiles around the river heads.
const OWNER: Pair = Pair {
    name: "owner",
    start: (996, 1040),
    goal: (1106, 930),
    bridges: &[((1016, 1020), (1021, 1015))],
};
/// One diagonal deck over a main river; no other way within hundreds of tiles.
const LOIRE: Pair = Pair {
    name: "loire",
    start: (960, 840),
    goal: (1120, 700),
    bridges: &[((1024, 756), (1029, 751))],
};
/// Two decks in sequence; 490 tiles without them.
const TWO: Pair = Pair {
    name: "two",
    start: (817, 1003),
    goal: (691, 1103),
    bridges: &[((743, 1062), (747, 1058)), ((703, 1116), (709, 1110))],
};
/// Three decks close together.
const THREE: Pair = Pair {
    name: "three",
    start: (927, 927),
    goal: (1100, 1040),
    bridges: &[
        ((937, 925), (940, 922)),
        ((972, 950), (976, 946)),
        ((984, 959), (988, 955)),
    ],
};
/// The same ends with no bridge: 332 tiles around the river heads.
const HEAD: Pair = Pair {
    name: "head",
    start: (927, 927),
    goal: (1100, 1040),
    bridges: &[],
};
/// Straight through a forest belt; the walk goes around (245 tiles for 185).
const FOREST: Pair = Pair {
    name: "forest",
    start: (1153, 1373),
    goal: (1327, 1437),
    bridges: &[],
};
/// A grove with no way through (156 tiles for 108).
const GROVE: Pair = Pair {
    name: "grove",
    start: (1610, 709),
    goal: (1718, 712),
    bridges: &[],
};
/// The owner's goal in its meander, reached by a deck that does lead there.
const MEANDER: Pair = Pair {
    name: "meander",
    start: (1010, 830),
    goal: (1106, 930),
    bridges: &[((1085, 920), (1090, 925))],
};
/// Scattered mountain trees (311 tiles for 215).
const PYRENEES: Pair = Pair {
    name: "pyrenees",
    start: (572, 1931),
    goal: (778, 1869),
    bridges: &[],
};

fn cell(pair: Pair, army: Army, general: bool) {
    if label().is_none() {
        return;
    }
    let _ = march(pair, army, general, setup(pair));
}

macro_rules! cells {
    ($($pair:ident $name:ident;)*) => {
        $(mod $name {
            use super::*;
            #[test] fn paladins() { cell($pair, Army::Paladins, false); }
            #[test] fn paladins_g() { cell($pair, Army::Paladins, true); }
            #[test] fn champions() { cell($pair, Army::Champions, false); }
            #[test] fn champions_g() { cell($pair, Army::Champions, true); }
            #[test] fn mixed() { cell($pair, Army::Mixed, false); }
            #[test] fn mixed_g() { cell($pair, Army::Mixed, true); }
            #[test] fn rams() { cell($pair, Army::Rams, false); }
            #[test] fn rams_g() { cell($pair, Army::Rams, true); }
        })*
    };
}

cells! {
    LOIRE loire;
    TWO two;
    THREE three;
    HEAD head;
    FOREST forest;
    GROVE grove;
    MEANDER meander;
    PYRENEES pyrenees;
}

/// 1,084 tiles: only the armies that matter for the owner's question.
mod owner {
    use super::*;
    #[test]
    fn paladins_g() {
        cell(OWNER, Army::Paladins, true);
    }
    #[test]
    fn mixed_g() {
        cell(OWNER, Army::Mixed, true);
    }
}
