//! The survey of a map for chokes, as text: `make game-score GAME_SCORE_ARGS='--survey-chokes'`.
//! It lists the necks an army has to use (closing the cross-section makes the way between two
//! anchors much longer, or closes it), narrowest cross-section and strongest detour first, with a
//! picture of each; `--survey-at x,y` describes the neck at a tile in full. This is how the choke
//! of a staged scenario was found, and how another is found (docs/scale/game-score-chokes.md).
use super::{
    Corridor, Grid, Ground, MapGround, Saddle, Tile, across, anchors, clearance, corridor, flanks,
    saddles, steps,
};
use crate::game_score::BoxError;
use aoe_server::HeadlessMatch;
use std::{fmt::Write as _, fs, path::Path};

/// The widest saddle looked at, in thirds of a tile of clearance (twelve tiles each side of it).
const WIDEST: u16 = 36;
/// The least ground, in tiles of clearance two or more, a stretch has to hold to count as one side.
const MIN_SIDE: usize = 400;
/// A neck narrower or wider than this (tiles between its ends) is not a choke for an army of
/// hundreds that has to file through it, or is no neck at all.
const WIDTHS: std::ops::RangeInclusive<u32> = 2..=20;
/// The anchors of the detour test: this far from the cross-section, along the march.
const ANCHOR_DISTANCE: i32 = 30;
/// A neck is kept when closing it makes the way between the anchors this much longer, or shuts it.
const MIN_DETOUR: f64 = 1.15;
/// Pictures are drawn for this many necks.
const PICTURES: usize = 25;

fn picture(ground: &dyn Ground, middle: Tile, reach: (i32, i32)) -> String {
    let mut out = String::new();
    for y in middle.1 - reach.1..=middle.1 + reach.1 {
        for x in middle.0 - reach.0..=middle.0 + reach.0 {
            out.push(if (x, y) == middle {
                'S'
            } else {
                ground.glyph((x, y))
            });
        }
        out.push('\n');
    }
    out
}

/// A neck kept by the survey: how much longer closing it makes the way, and what it measures.
struct Kept {
    detour: f64,
    saddle: Saddle,
    neck: Corridor,
    text: String,
}

fn keep(ground: &dyn Ground, grid: &Grid, clear: &[u16], saddle: Saddle) -> Option<Kept> {
    let neck = corridor(&|tile| grid.walkable(tile), saddle.tile)?;
    if !WIDTHS.contains(&neck.width) {
        return None;
    }
    let (near, far) = anchors(grid, clear, &neck, ANCHOR_DISTANCE)?;
    let through = steps(grid, near, far, &[]);
    let closed = steps(grid, near, far, &across(neck.ends));
    let detour = match (through, closed) {
        (Some(through), Some(closed)) => f64::from(closed) / f64::from(through.max(1)),
        (Some(_), None) => 99.0,
        _ => return None,
    };
    (detour >= MIN_DETOUR).then(|| Kept {
        detour,
        saddle,
        neck,
        text: format!(
            "anchors {near:?} {far:?}, {through:?} steps through, {closed:?} with the neck closed, flanks {}",
            flanks(ground, &neck)
        ),
    })
}

/// Everything the record of one neck says, measured again: the neck, the anchors at several
/// distances, the way between them with and without the neck, where the barrier along the
/// cross-section's own line is open, and a picture.
fn detail(grid: &Grid, clear: &[u16], ground: &dyn Ground, saddle: Tile) -> String {
    let mut out = format!("\nDETAIL {saddle:?}\n");
    let Some(neck) = corridor(&|tile| grid.walkable(tile), saddle) else {
        return out + "no neck\n";
    };
    let _ = writeln!(out, "{neck:?} flanks {}", flanks(ground, &neck));
    for distance in [20, 30, 40, 50] {
        if let Some((near, far)) = anchors(grid, clear, &neck, distance) {
            let _ = writeln!(
                out,
                "anchors at {distance}: {near:?} {far:?}, {:?} steps through, {:?} with the neck closed",
                steps(grid, near, far, &[]),
                steps(grid, near, far, &across(neck.ends))
            );
        }
    }
    let line = (neck.flow.1, neck.flow.0);
    let mut runs: Vec<String> = Vec::new();
    let mut open: Option<(Tile, i32)> = None;
    for step in -90..=90 {
        let tile = (saddle.0 + step * line.0, saddle.1 + step * line.1);
        match (grid.walkable(tile), open) {
            (true, None) => open = Some((tile, 1)),
            (true, Some((from, n))) => open = Some((from, n + 1)),
            (false, Some((from, n))) => {
                runs.push(format!("{from:?}x{n}"));
                open = None;
            }
            (false, None) => {}
        }
    }
    let _ = writeln!(
        out,
        "walkable runs along the cross-section's line (90 tiles either way, start and length): {}",
        runs.join(" ")
    );
    out + &picture(ground, saddle, (60, 40))
}

/// The survey of `ground`: the necks kept, strongest detour first, then a picture of the first
/// few and the detail of every tile in `details`.
pub(in crate::game_score) fn survey(ground: &dyn Ground, details: &[Tile]) -> String {
    let began = std::time::Instant::now();
    let grid = Grid::of(ground);
    let clear = clearance(&grid);
    let found = saddles(&grid, &clear, WIDEST, MIN_SIDE);
    // One saddle for every sixteen tiles: neighbours are one neck seen twice.
    let mut distinct: Vec<Saddle> = Vec::new();
    for saddle in found.iter().copied() {
        let near = |other: &Saddle| {
            (other.tile.0 - saddle.tile.0)
                .abs()
                .max((other.tile.1 - saddle.tile.1).abs())
                < 16
        };
        if !distinct.iter().any(near) {
            distinct.push(saddle);
        }
    }
    let mut kept: Vec<Kept> = distinct
        .iter()
        .filter_map(|&saddle| keep(ground, &grid, &clear, saddle))
        .collect();
    kept.sort_by(|a, b| b.detour.total_cmp(&a.detour));
    let mut out = format!(
        "{side} tiles a side, {} saddles up to {WIDEST} thirds of a tile with {MIN_SIDE} tiles of ground on each side, {} of them distinct, {} kept: closing the cross-section makes the way between anchors {ANCHOR_DISTANCE} tiles either side {MIN_DETOUR} times longer or shuts it ({:.1} s)\n",
        found.len(),
        distinct.len(),
        kept.len(),
        began.elapsed().as_secs_f64(),
        side = grid.side(),
    );
    for (number, one) in kept.iter().enumerate() {
        let _ = writeln!(
            out,
            "#{number} detour {:.2} {:?} clearance {:.1} tiles, ground {} and {} tiles, width {} length {} ends {:?} flow {:?}\n    {}",
            one.detour,
            one.saddle.tile,
            f64::from(one.saddle.clearance) / 3.0,
            one.saddle.sides.0,
            one.saddle.sides.1,
            one.neck.width,
            one.neck.length,
            one.neck.ends,
            one.neck.flow,
            one.text,
        );
    }
    for (number, one) in kept.iter().enumerate().take(PICTURES) {
        let _ = writeln!(out, "\n#{number} {:?}", one.saddle.tile);
        out.push_str(&picture(ground, one.saddle.tile, (40, 30)));
    }
    for &tile in details {
        out.push_str(&detail(&grid, &clear, ground, tile));
    }
    out
}

/// Surveys the map `hash` of `directory` and writes the text to `out`.
pub(in crate::game_score) fn survey_map(
    directory: &Path,
    hash: &str,
    details: &[Tile],
    out: &Path,
) -> Result<(), BoxError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let world = runtime.block_on(HeadlessMatch::bare_world(directory, hash))?;
    let ground = MapGround::of(&world).ok_or("the package is not a map")?;
    fs::write(out, survey(&ground, details))?;
    eprintln!(
        "game score: wrote the survey of the chokes to {}",
        out.display()
    );
    Ok(())
}
