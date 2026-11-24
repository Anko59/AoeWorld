//! Parity tests (#978, 4.3): a scene runs `RUNS` times with its start
//! positions jittered by up to half a tile, once watched and once unwatched,
//! and the means of its measures are compared. The level of detail a unit
//! gets may depend on who looks; who wins a fight may not.
use super::*;
use crate::game_world::terrain_fixtures::build;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileRect};

mod fights;
mod gate;
mod march;

/// Sixteen runs: a fight scene can end in one of two outcomes (test 3: the
/// archer duel is won by one side or the other), and with eight runs one
/// run changing outcome moved a mean by 0.125, over the 10% bound alone
/// (11-24: watched 2 of 8 against unwatched 3 of 8 lost duels).
const RUNS: u64 = 16;

/// Both runs make every player of the scene a human seat, so automation
/// treats them alike; only what the clients show differs. `Watched`: every
/// client shows the whole map at unit zoom, so all of the scene runs the
/// full model. `Unwatched`: every client shows a far corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Watched,
    Unwatched,
}

impl Look {
    fn seat(self, world: &mut GameWorld, players: &[PlayerId]) {
        world.set_human_seats(players);
        let side = world.config.width_tiles.max(world.config.height_tiles);
        let region = match self {
            Self::Watched => TileRect::from_xywh(0, 0, side, side),
            Self::Unwatched => TileRect::from_xywh(0, 0, 8, 8),
        };
        let views: Vec<_> = players
            .iter()
            .map(|&player| crate::HumanView {
                player,
                region,
                unit_zoom: true,
            })
            .collect();
        world.set_human_views(&views);
    }
}

/// Seeded start positions: a tile centre moved by up to half a tile on
/// each axis.
struct Jitter(u64);

impl Jitter {
    fn at(&mut self, x: i32, y: i32) -> WorldPosition {
        let centre = center(x, y);
        let half = FIXED_SUBUNITS_PER_TILE / 2;
        let mut offset = || {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % (2 * half as u64 + 1)) as i32 - half
        };
        WorldPosition::new(centre.x + offset(), centre.y + offset())
    }
}

/// Mean and standard error of each measure over the runs.
#[derive(Debug)]
struct Means(Vec<(f64, f64)>);

fn means(look: Look, scene: &impl Fn(Look, &mut Jitter) -> Vec<f64>) -> Means {
    let runs: Vec<Vec<f64>> = (0..RUNS)
        .map(|run| scene(look, &mut Jitter(0x9e37_79b9_7f4a_7c15 ^ (run + 1))))
        .collect();
    let n = RUNS as f64;
    Means(
        (0..runs[0].len())
            .map(|i| {
                let mean = runs.iter().map(|r| r[i]).sum::<f64>() / n;
                let var = runs.iter().map(|r| (r[i] - mean).powi(2)).sum::<f64>() / (n - 1.0);
                (mean, (var / n).sqrt())
            })
            .collect(),
    )
}

/// Runs `scene` watched and unwatched. Measures are fractions (of the hit
/// points, units or time the scene starts with), so the bounds read as
/// shares of it.
fn compare(
    name: &str,
    measures: &[&str],
    scene: impl Fn(Look, &mut Jitter) -> Vec<f64>,
) -> (Means, Means) {
    let (watched, unwatched) = (means(Look::Watched, &scene), means(Look::Unwatched, &scene));
    for (i, measure) in measures.iter().enumerate() {
        let ((w, ws), (u, us)) = (watched.0[i], unwatched.0[i]);
        eprintln!(
            "parity {name} {measure}: watched {w:.3} (se {ws:.3}), unwatched {u:.3} (se {us:.3})"
        );
    }
    (watched, unwatched)
}

/// One-sided, for movement (lower is better): unwatched no more than 10%
/// worse than watched. Unwatched being better is expected (#978 4.3).
fn assert_one_sided(name: &str, ((w, _), (u, _)): ((f64, f64), (f64, f64))) {
    assert!(
        u <= w + 0.10,
        "{name}: unwatched {u:.3} against watched {w:.3}"
    );
}

/// Two-sided: each difference under 10%, and under the larger of 3% and
/// twice its standard error.
fn assert_two_sided(name: &str, measures: &[&str], (watched, unwatched): (Means, Means)) {
    for (i, measure) in measures.iter().enumerate() {
        let ((w, ws), (u, us)) = (watched.0[i], unwatched.0[i]);
        let (gap, se) = ((w - u).abs(), (ws * ws + us * us).sqrt());
        assert!(
            gap < 0.10 && gap < 0.03_f64.max(2.0 * se),
            "{name} {measure}: watched {w:.3} against unwatched {u:.3} (se {se:.3})"
        );
    }
}
