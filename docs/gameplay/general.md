# The General

The General is an original mounted army leader, trained at a completed Castle
in Castle Age for 800 food and 800 gold in 60 seconds, one population slot.
Its 350 HP, 22 melee attack, 5 melee armour, 6 pierce armour, 1.8-second attack
period and 1.40-tile/second speed make it a strong fighter. There is no
one-per-Castle restriction. Ordinary population, training and cancellation
rules apply. The Steward is the economy specialist.

## Command and production

Creation adopts up to 100 living owned land soldiers within 24 tiles, in entity
order, excluding civilians, monks, ships and other leaders. An existing
General's soldiers are never stolen. Later surveys only observe enemies;
ordinary General orders never recruit free or owner-commanded soldiers.
The limit includes prepaid reinforcements. Death releases command immediately;
sheltered/transported soldiers retain their slots but receive no field orders.
Adoption alone leaves soldiers in place; a march order, combat or retreat
decision starts field movement. A newly trained commander does not pull the
army back into its Castle exit before the player's first order.

A new General trains nothing. After the **Train** button (`GeneralCommand::SetTraining`;
SDK `Order::GeneralTrain`), an admitted decision with space remaining may buy one
ordinary unit from an idle completed owned production building within the
influence sphere. When the army and queued recruits reach 100, training switches
itself off and stays off, even after losses, until Train is pressed again. The
**unit-mix sliders** (`GeneralCommand::SetMix`, SDK `Order::GeneralMix`, 0 to 100
per role: infantry, archers, heavy cavalry, siege, scouts; default 35:30:20:10:5)
set the ratio the manager buys toward; a role at zero is never bought. Upgrades
follow the current research roster. Unavailable or unaffordable roles fall
through to another available role.
Barracks, Ranges, Stables and Workshops supply the current military roster;
Castles currently train leaders, so they do not supply ordinary reinforcements.
The manager does not build missing production buildings or research upgrades.
Queue reservations route new recruits directly to their General, regardless of
spawn distance; cancellation, building destruction and General death retire
reservations. Existing manual queues and rallies remain ordinary gameplay.

## The General's panel

Selecting one owned General shows a panel above the command bar with two modes;
a General starts in **Captain** mode. Captain shows what he manages: soldiers
by role, and the Marching or Fighting sub-mode he is currently in. **Trainer**
shows the five unit-mix sliders and the Train button with a note (idle, training
toward 100, or at capacity). The mode is a client convenience; the sliders, the
button and the soldier counts are authoritative server state
(`GeneralStatus.training/mix/counts/queued`).

## Influence sphere

A General's influence reaches 50 tiles (`GENERAL_INFLUENCE_TILES`). New march
orders reach soldiers inside it. An admitted march continues through its
intermediate legs even if the commander and a trailing squad temporarily become
farther apart. Production buildings must lie inside the influence sphere.

## Orders

Right-clicking ground leads the army (above). The **General only** button or
Alt+right-click walks the General alone (`GeneralOrder::MoveAlone`; Rust SDK
`Order::GeneralMove`): the army stops where it stands and is not led or retreated
until the next army march or Stop, and the General is no longer in the shared
block. He uses the same offensive movement and friendly collision recovery as any
other single-unit group.

## March and combat

Right-clicking ground with a General selected sends an ordinary move order.
The server retains that destination as the army's march goal. Stop cancels the
army march. Each squad uses the ordinary formation routes, corridor following,
obstruction handling, collision, path budgets and per-member arrival. Terrain
can squeeze a squad into narrow files. At the objective, squad destinations
place infantry ahead of archers, siege behind archers, Knights on the flanks and
scouts/light cavalry ahead of the infantry. Shared formation assignment lays
out each selection, including selections containing the General; army ownership
does not select a different layout.

Every decision compares nearby visible enemy HP/attack/cooldown strength with
its whole army (every assigned member, wherever it stands). Defensive Castles and towers add a fixed threat allowance.
An enemy advantage above 25%, a severely depleted engaged army, or a wounded
General triggers retreat toward the Castle
that trained it (nearest completed Castle for scenario-spawned Generals).
The Castle's outside rendezvous survives destruction. Retreat lasts at least
five seconds; once safe, the army regroups and resumes its retained destination.

A march is an attack-move formation for each typed squad and for the General
himself, through the same order API a player's group selection uses. Spotting an
enemy changes the panel's state, not the admitted march: the ordinary formation
engage look picks reachable enemies inside each soldier's own sight and resumes
the soldier's march after combat. There is no separate General firing-position
search or army-wide target priority. Siege uses the same acquisition as a
player's attack-move group, including buildings.

The slowest squad's ordinary route supplies a 64-tile intermediate destination
for the other squads and the commander, followed by their final destinations
in the ordinary offensive waypoint FIFO. The shared anchor continues through
the waypoint without rebuilding a squad's physical centroid midway across a
bridge. Squads already within 64 tiles finish their own approach, so a commander
already across a river is not sent back toward the crossing. Destination
projection and terrain recovery belong to the shared formation code.

During intermediate legs squads coordinate at the slowest army member's pace
through ordinary formation pace control, which can coordinate any player's
groups. On final approaches they catch up at their own pace; a squad more than
twelve tiles ahead of the commander uses the escort pace until he catches up.
The commander keeps that escort pace. Movement has no collision exemption or
General lookup. Front members respect that pace; only trailing members spend
catch-up credit. Queued intermediate destinations are passing points, and their
shared corridor remains available to the rear after the anchor turns.

After every admitted march and fight has finished, a still-visible contact may
receive another ordinary offensive click. This clears contacts passed by the
first march without replacing an underway order or assigning individual targets.

Marching preserves the player's stances. Retreat retains a temporary Defensive
stance to limit autonomous pursuit while falling back. Units below one-third
HP withdraw toward home with NoAttack; their retained movement goes through
the ordinary move-goal API. A rotating 16-member window bounds wounded-unit
commands. The original stance returns when its override ends, including
healing, Stop, release, death or sheltering/transport of the commander.
Restoration covers the whole army independently of that window.
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
most 16 withdrawal commands and 21 formation/waypoint commands,
plus one training order.
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
slots, ordinary siege acquisition, wounded withdrawal, retreat, stop, death and
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
