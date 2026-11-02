//! Shared deployment for wall-clock and movement-only Callgrind measurements.
use aoe_core::{Formation, PlayerId, Seed, WorldConfig, WorldPosition};
use aoe_simulation::{GameWorld, UnitKind, WorldLimits};
type BoxError = Box<dyn std::error::Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Parameters {
    pub(super) armies: u32,
    pub(super) min_size: u32,
    pub(super) max_size: u32,
    pub(super) cell_tiles: i32,
    pub(super) side_tiles: u32,
    pub(super) players: u16,
    pub(super) seed: u64,
    pub(super) fog_off: bool,
    pub(super) attack_move: bool,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            armies: 200,
            min_size: 30,
            max_size: 79,
            cell_tiles: 24,
            side_tiles: 1000,
            players: 100,
            seed: 1,
            fog_off: false,
            attack_move: false,
        }
    }
}

fn at(x: i32, y: i32) -> WorldPosition {
    // The centre of tile (x, y): 1,024 subunits per tile.
    WorldPosition::new(x * 1024 + 512, y * 1024 + 512)
}

/// Each army marches to the mirror cell inside its block of four columns, so
/// the pairs 0-3 and 1-2 pass through each other; every third army also
/// crosses to the other row of its pair of rows.
fn partner(index: u32, columns: u32) -> u32 {
    let (cx, cy) = (index % columns, index / columns);
    let py = if index.is_multiple_of(3) { cy ^ 1 } else { cy };
    py * columns + (cx ^ 3)
}

pub(super) struct Armies {
    pub(super) ids: Vec<Vec<aoe_core::EntityId>>,
    pub(super) orders_ok: u32,
    pub(super) spawned: u32,
    pub(super) refused: std::collections::BTreeMap<String, u32>,
}

/// Trees and water close some tiles: try the nearest others.
fn nearby(x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
    (0..64).map(move |k| (x + k % 8 * (1 - k / 8 % 2 * 2), y + k / 8))
}

fn deploy(world: &mut GameWorld, o: &Parameters) -> Result<Armies, BoxError> {
    let columns = 20_u32;
    let rows = o.armies.div_ceil(columns);
    let (cell, side) = (o.cell_tiles, o.side_tiles as i32);
    let origin = (
        (side - columns as i32 * cell) / 2,
        (side - rows as i32 * cell) / 2,
    );
    let corner = |i: u32| {
        (
            origin.0 + (i % columns) as i32 * cell,
            origin.1 + (i / columns) as i32 * cell,
        )
    };
    let mut out = Armies {
        ids: Vec::new(),
        orders_ok: 0,
        spawned: 0,
        refused: Default::default(),
    };
    for i in 0..o.armies {
        let player = PlayerId((i % u32::from(o.players)) as u16);
        let size = o.min_size + (i * 17) % (o.max_size - o.min_size + 1);
        let (x0, y0) = corner(i);
        let mut ids = Vec::new();
        for n in 0..size as i32 {
            let wanted = (x0 + n % 10, y0 + n / 10);
            let spawned = nearby(wanted.0, wanted.1)
                .find_map(|(x, y)| world.spawn_kind(player, UnitKind::Militia, at(x, y)).ok());
            ids.extend(spawned);
        }
        out.spawned += ids.len() as u32;
        out.ids.push(ids);
    }
    for i in 0..o.armies {
        let ids = &out.ids[i as usize];
        if ids.is_empty() {
            continue;
        }
        let (x1, y1) = corner(partner(i, columns).min(o.armies - 1));
        let player = PlayerId((i % u32::from(o.players)) as u16);
        let mut last = None;
        let ok = nearby(x1 + 4, y1 + 4).any(|(x, y)| {
            let sent = world.issue_formation_move(
                player,
                ids,
                Formation::Line,
                at(x, y),
                false,
                o.attack_move,
            );
            last = sent.err();
            last.is_none()
        });
        out.orders_ok += u32::from(ok);
        if let Some(error) = last.filter(|_| !ok) {
            *out.refused.entry(format!("{error:?}")).or_default() += 1;
        }
    }
    Ok(out)
}

pub(super) fn build(o: &Parameters) -> Result<(GameWorld, Armies), BoxError> {
    let mut world = GameWorld::new(WorldConfig::new(
        o.side_tiles as i32,
        o.side_tiles as i32,
        Seed(o.seed),
    )?)?;
    world.set_limits(WorldLimits {
        players: 4_096,
        buildings: 1 << 20,
        gatherers: 1 << 20,
        builders: 1 << 20,
        hunters: 1 << 20,
        animals: 1 << 20,
        projectiles: 1 << 20,
    });
    world.set_fog_disabled(o.fog_off);
    let armies = deploy(&mut world, o)?;
    Ok((world, armies))
}
