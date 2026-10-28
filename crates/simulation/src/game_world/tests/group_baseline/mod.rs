//! PR 0 of the group-movement rewrite (`docs/movement/group-movement-rewrite.md`
//! section 6): one scenario per owner symptom. Each test records what today's
//! code measures and asserts it exactly. Where the value is wrong by the
//! design's standard, a comment names the target and the PR that flips it.
//! Ticks are 20 Hz; positions are 1,024 subunits per tile.
use super::far_orders::{center, wide_world};
use super::*;
use aoe_core::{CombatStance, Formation};

mod arrival;
mod clash;
mod open_ground;
mod reorder;

/// `count` militia of `player` in rows of `columns`, one tile apart.
pub(super) fn block(
    w: &mut GameWorld,
    player: PlayerId,
    (x, y): (i32, i32),
    columns: usize,
    count: usize,
) -> Vec<EntityId> {
    (0..count)
        .map(|n| {
            let at = center(x + (n % columns) as i32, y + (n / columns) as i32);
            w.spawn_kind(player, UnitKind::Militia, at).unwrap()
        })
        .collect()
}

/// Trees: closed to walking and to route planning, as a generated forest is.
pub(super) fn forest(w: &mut GameWorld, rect: TileRect) {
    if let Terrain::Map { overlay, .. } = &mut w.terrain {
        overlay.add_footprint(rect).unwrap();
    }
    w.collision_closed(rect);
}

pub(super) fn tiles_between(a: WorldPosition, b: WorldPosition) -> i64 {
    let (x, y) = (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    );
    (x * x + y * y).isqrt() / 1024
}

/// When each unit first stopped, and where the others had got to by then.
#[derive(Debug, Default)]
pub(super) struct March {
    pub idle_at: Vec<Option<u32>>,
    /// Units that stopped more than eight tiles from the destination.
    pub stopped_short: usize,
    /// Ticks the group spent in column mode (single file).
    pub column_ticks: u32,
    /// Narrowest frontage (tiles across the heading) seen while the longest
    /// stretch along the heading exceeded twenty tiles.
    pub min_frontage_when_long: i64,
    pub longest_tiles: i64,
}

impl March {
    pub fn fields(&self) -> [(&'static str, i64); 7] {
        [
            ("first_stop", self.first().into()),
            ("last_stop", self.last().into()),
            ("stopped", self.stopped() as i64),
            ("stopped_short", self.stopped_short as i64),
            ("column_ticks", self.column_ticks.into()),
            ("longest_tiles", self.longest_tiles),
            ("min_frontage_when_long", self.min_frontage_when_long),
        ]
    }

    pub fn first(&self) -> u32 {
        self.idle_at.iter().flatten().copied().min().unwrap_or(0)
    }
    pub fn last(&self) -> u32 {
        self.idle_at.iter().flatten().copied().max().unwrap_or(0)
    }
    pub fn stopped(&self) -> usize {
        self.idle_at.iter().flatten().count()
    }
}

/// Advance until every unit has stopped or `limit` ticks pass. The heading is
/// east: along is x, across is y.
pub(super) fn march(
    w: &mut GameWorld,
    ids: &[EntityId],
    destination: WorldPosition,
    limit: u32,
) -> March {
    let mut out = March {
        idle_at: vec![None; ids.len()],
        min_frontage_when_long: i64::MAX,
        ..Default::default()
    };
    for tick in 1..=limit {
        w.advance();
        let (mut min, mut max) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
        for (n, &id) in ids.iter().enumerate() {
            let u = w.unit(id).unwrap();
            min = (min.0.min(u.position.x), min.1.min(u.position.y));
            max = (max.0.max(u.position.x), max.1.max(u.position.y));
            if out.idle_at[n].is_none() && !u.moving {
                out.idle_at[n] = Some(tick);
                if tiles_between(u.position, destination) > 8 {
                    out.stopped_short += 1;
                }
            }
        }
        let along = i64::from(max.0 - min.0) / 1024;
        let across = i64::from(max.1 - min.1) / 1024;
        out.longest_tiles = out.longest_tiles.max(along);
        if along > 20 {
            out.min_frontage_when_long = out.min_frontage_when_long.min(across);
        }
        out.column_ticks += u32::from(w.formation_is_column(ids[0]) == Some(true));
        if out.stopped() == ids.len() {
            break;
        }
    }
    out
}

/// Attackers of every hit since the last call. A victim keeps only its latest
/// hit, so two blows in one tick count once: the totals are lower bounds.
pub(super) fn new_hits(
    w: &GameWorld,
    victims: &[EntityId],
    seen: &mut std::collections::BTreeMap<EntityId, Tick>,
) -> Vec<EntityId> {
    let mut out = Vec::new();
    for &v in victims {
        // A corpse is removed after thirty seconds; nothing more to report.
        if let Some(hit) = w.unit(v).and_then(|u| u.last_hit)
            && seen.insert(v, hit.tick) != Some(hit.tick)
        {
            out.push(hit.attacker);
        }
    }
    out
}

/// Print the numbers a later PR must beat; `--no-capture` shows them.
pub(super) fn note(name: &str, fields: &[(&str, i64)]) {
    let line = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("group-baseline {name}: {line}");
}
