# The General

The General is an original mounted army leader, trained at a completed Castle
in Castle Age for 800 food and 800 gold in 60 seconds, one population slot.
Its 350 HP, 22 melee attack, 5 melee armour, 6 pierce armour, 1.8-second attack
period and 1.40-tile/second speed make it a strong fighter. There is no
one-per-Castle restriction. Ordinary population, training and cancellation
rules apply. The Vassal remains the more expensive economy specialist.

## Command and production

Creation adopts up to 100 living owned land soldiers within 24 tiles, in entity
order, excluding civilians, monks, ships and other leaders. An existing
General's soldiers are never stolen. Later bounded censuses fill vacancies.
The limit includes prepaid reinforcements. Death releases command immediately;
sheltered/transported soldiers retain their slots but receive no field orders.
Adoption alone leaves soldiers in place; a march order, combat or retreat
decision starts field movement. A newly trained commander does not pull the
army back into its Castle exit before the player's first order.

When space remains, an admitted decision may buy one ordinary unit from an
idle completed owned production building. Infantry:archers:heavy cavalry:siege:
scouts aim for 35:30:20:10:5. Upgrades follow the current research roster.
Unavailable or unaffordable roles fall through to another available role.
Barracks, Ranges, Stables and Workshops supply the current military roster;
Castles currently train leaders, so they do not supply ordinary reinforcements.
The manager does not build missing production buildings or research upgrades.
Queue reservations route new recruits directly to their General, regardless of
spawn distance; cancellation, building destruction and General death retire
reservations. Existing manual queues and rallies remain ordinary gameplay.

## March and combat

Right-clicking ground with a General selected sends an ordinary move order.
The server retains that destination as the army's march goal. Stop cancels the
army march. Shared formation routes, obstruction handling, block movement,
collision, path budgets and combat execution are the existing systems. The
block follows a straightened corridor that keeps clear of banks and forest
edges, with a proportional cohesion reserve ([real terrain](../movement/real-terrain.md)):
100 followers cross a river ford within 1.3 times their slowest member's route.
Role slots place infantry ahead of archers, siege behind archers, Knights on
the flanks and scouts/light cavalry ahead of the infantry. Terrain can squeeze
the block into narrow files. Role destinations use the block's ordinary body
spacing, clearance validation and per-member arrival; no member pathfinds
unless detached by the shared obstruction handling.

Every decision compares nearby visible enemy HP/attack/cooldown strength with
its own army within sixteen tiles, so distant recruits do not inflate strength. Defensive Castles and towers add a fixed threat allowance.
An enemy advantage above 25%, a severely depleted engaged army, a wounded
General, or an incomplete crowded census triggers retreat toward the Castle
that trained it (nearest completed Castle for scenario-spawned Generals).
The Castle's outside rendezvous survives destruction. Retreat lasts at least
five seconds; once safe, the army regroups and resumes its retained destination.

In a favourable fight, soldiers focus visible siege (including Mangonels and
Battering Rams), then Monks, then leaders (Generals and Vassals), then all
archer kinds (including Skirmishers), then other wounded targets. Within each
class, the lowest remaining HP fraction wins, then distance and entity ID
resolve ties. Siege prefers
visible buildings. Units below one-third HP withdraw toward home. A rotating
16-member micro window prevents a large army from monopolising orders.
The General records each soldier's original stance before a march, retreat or
wounded withdrawal overrides it. The original stance returns when that reason
ends, including healing, arrival, fighting, regrouping without a destination,
Stop, release, death or sheltering/transport of the commander. Restoration
covers the whole army independently of the combat micro window.
The estimator is deliberately simple, with no opponent-state or fog bypass.

## Determinism and work limits

All state uses integers and ordered indexes and participates in canonical
hashing: homes, destinations, membership, queued recruits, scan cursors, rotation
and retreat deadlines, plus the original stances and override reasons in 101
inline slots per army. Stance bookkeeping allocates nothing per decision.
No randomness, wall clock, network or client decisions.

At 20 Hz, admission runs every ten ticks. At most eight Generals are admitted,
rotating by entity ID. Each scans at most 1,024 spatial unit entries, 625 nearby
building-occupancy tiles and 128 owner production buildings; each issues at
most 16 combat commands, or one shared formation, plus one training order.
Partial spatial censuses resume on the next admission and conservatively
refuse a fight. The global census bounds per admission pass are 8,192 unit
entries and 6,024 building/tile visits. Formation membership is at most 101.
More than eight active Generals share the admission budget; their decisions
become less frequent. This bounds work, but is not 1,000-player qualification.

The HUD shows the selected owned General's army size and idle (no march
destination), marching, fighting or retreating state. Commander, home and summary fields are owner-only. Rust and
Python SDKs expose the current General unit identity and summary; ordinary move
and stop orders control it. The wire has one current format, with no migration.

## Licensed appearance and validation

The General uses the DE Frankish Paladin hero's separate idle, walk, attack,
death and decay graphics (`u_cav_hero_frankish_paladin_*A_x1.sld`). The importer
preserves all 3,125 source cells and their colour, player and shadow planes.
The runtime selects the sixteen direction blocks: 45 cells per idle/attack/death
and 30 per walk/decay (3,120 cells). Movement selects native headings from the
actual interpolated movement segment, without mirroring the hero's equipment.
All five poses have distinct renderer tags; an unknown tag cannot turn the
General into the Scout.

The DE DAT supplies per-cell durations: idle 77.7778 ms, walk 36.6667 ms,
attack 35.5556 ms, death 41.1111 ms and decay 1,000 ms. Import validates sprite
identities, frame/angle counts and fractional durations against that data;
unsupported changes fail explicitly. Playback retains fractional milliseconds,
and one-shot death playback uses the interpolated tick between snapshots.
The x1 files are the highest available resolution for this hero in the owner's
install; no x2 hero files exist. Conversion crops transparent margins and moves
the hotspot without resampling the image. Source and derived images remain
outside Git and container images.

The shared atlas grows in colour/mask pairs when selected native art needs more
space. It retains the base eight layers and allocates up to 64 layers, with
65,536 semantic frame entries; owner colours share one copy of the artwork.
WebGPU uploads one 16 MiB page at a time. WebGL2 and Canvas retain their bounded
source atlas for restoration/rasterization. These are startup selection bounds,
not visibility-based eviction. See [DE conversion](../assets/de-full.md) for
measurement and remaining conversion work.

Simulation tests cover takeover, production balance and reservations, role
slots, siege building preference, wounded withdrawal, retreat, stop, death and
replay. Protocol tests pin the current wire and privacy; SDK rules share the
server tables. Browser tests use synthetic manifests. Instruction comparisons
on smoke_ticks, distributed_ticks and late_game measure unchanged synthetic
workloads; they do not establish finished RTS or dedicated hardware performance.

For a repeatable local capture, `POST /maps/reset?scenario=general-army`
prepares a Castle, unclaimed soldiers of every role and five distant enemy
militia. Open `/?play=1`, train the General at the Castle, then march toward
tile (166, 132). This explicit showcase runs the ordinary game at four times
speed (the same clock convention as `feudal-military`); it does not change
training costs, orders, acquisition, movement or combat rules.
