//! Parity tests (#978, 4.3): a scene runs `RUNS` times with its start
//! positions jittered by up to half a tile, once watched and once unwatched,
//! and the means of its measures are compared. The level of detail a unit
//! gets may depend on who looks; who wins a fight may not.
use super::*;
use crate::game_world::terrain_fixtures::build;
use aoe_core::FIXED_SUBUNITS_PER_TILE;

mod fights;

const RUNS: u64 = 8;

/// `Watched`: every player of the scene is a human seat, so all of it runs
/// the full model. `Unwatched`: the only human seat is player 0, who owns
/// nothing and sees nothing of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Watched,
    Unwatched,
}

impl Look {
    fn seat(self, world: &mut GameWorld, players: &[PlayerId]) {
        match self {
            Self::Watched => world.set_human_seats(players),
            Self::Unwatched => world.set_human_seats(&[PlayerId(0)]),
        }
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
