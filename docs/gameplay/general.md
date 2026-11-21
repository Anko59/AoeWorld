# The General

The General is an original mounted army leader, trained at a completed Castle
in Castle Age for 800 food and 800 gold in 60 seconds, one population slot.
Its 350 HP, 22 melee attack, 5 melee armour, 6 pierce armour, 1.8-second attack
period and 1.40-tile/second speed make it a strong fighter. There is no
one-per-Castle restriction. Ordinary population, training and cancellation
rules apply. The Steward is the economy specialist.

## Command and production

Creation adopts up to 200 living owned land soldiers within 24 tiles, subject
to the ten squad offices, in entity order, excluding civilians, monks, ships
and other leaders. An existing
General's soldiers are never stolen. Later surveys only observe enemies;
ordinary General orders never recruit free or owner-commanded soldiers.
The limit includes prepaid reinforcements. Death releases command immediately;
sheltered/transported soldiers retain their slots but receive no field orders.
Adoption alone leaves soldiers in place; a march order, combat or retreat
decision starts field movement. A newly trained commander does not pull the
army back into its Castle exit before the player's first order.

A new General trains nothing. After the **Train** button (`GeneralCommand::SetTraining`;
SDK `Order::GeneralTrain`), each admission can buy one ordinary unit per
eligible building in the current scan slice. Training uses idle, completed, owned production buildings within
the influence sphere. The scan rotates so later buildings get their turn.
If the enabled roles have no squad vacancies left after reservations, training
switches itself off and stays off until Train is pressed again. The
**unit-mix sliders** (`GeneralCommand::SetMix`, SDK `Order::GeneralMix`, 0 to 100
per role: infantry, archers, heavy cavalry, siege, scouts; default 35:30:20:10:5)
set the ratio the manager buys toward; a role at zero is never bought. Upgrades
follow the current research roster. Unavailable or unaffordable roles fall
through to another available role.
Barracks, Ranges, Stables, Workshops and Castles supply eligible units from
their current military rosters. Recruiting excludes other leaders.
The manager does not build missing production buildings or research upgrades.
Queue reservations route new recruits directly to their General, regardless of
spawn distance; cancellation, building destruction and General death retire
reservations. Existing manual queues and rallies remain ordinary gameplay.

## The General's panel

Selecting one owned General shows a panel above the command bar with two modes;
a General starts in **Captain** mode. Captain shows what he manages: soldiers
by role, and the Marching or Fighting sub-mode he is currently in. **Trainer**
shows the five unit-mix sliders, the Train button and the queued recruit count.
The mode is a client convenience. The sliders, the button and the soldier
counts are authoritative server state
(`GeneralStatus.training/mix/counts/queued`).

## Training radius

Production buildings must be within 50 tiles of the General
(`GENERAL_INFLUENCE_TILES`). Army orders reach all assigned soldiers on the
field, including squads farther away from the General.

## Orders

Selecting a General with its soldiers keeps their membership for move, attack,
and attack-move. An enemy click focuses the army on that target. Shift queues
ground orders on the army. The next leg starts after the commander and squads
finish the current one. Move and attack-move keep their individual behavior
through saves and restores. Selecting a soldier without its own General still
detaches it. A Vassal can call the General for defence after the owner's active
command finishes.

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
scouts/light cavalry ahead of the infantry. Each squad uses the shared formation
layout. Independent soldiers in the same selection use the player's chosen
formation.

When the army is under automatic control, the General compares nearby visible
enemy HP, attack and cooldown with its whole army. Defensive Castles and towers
add a fixed threat allowance. An enemy advantage above 25%, a severely depleted
engaged army, or a wounded General triggers retreat toward the Castle
that trained it (nearest completed Castle for scenario-spawned Generals).
The Castle's outside rendezvous survives destruction. Retreat lasts at least
five seconds; once safe, the army regroups and resumes its retained destination.

A march uses ordinary formations for the General and each squad. Plain moves
keep their destination when the army sees a threat. On an attack-move, a squad
can engage a nearby enemy spotted by its General or seen by the soldier itself.
A soldier that breaks off for combat resumes the march afterward. Siege uses
the same acquisition as other attack-move formations, including buildings.

The slowest squad's ordinary route supplies a 64-tile intermediate destination
for the other squads and the commander, followed by their final destinations
in the ordinary formation waypoint queue. The shared anchor continues through
the waypoint without rebuilding a squad's physical centroid midway across a
bridge. Squads already within 64 tiles finish their own approach, so a commander
already across a river is not sent back toward the crossing. Destination
projection and terrain recovery belong to the shared formation code.

During intermediate legs squads coordinate at the slowest army member's pace
through ordinary formation pace control, which can coordinate any player's
groups. On final approaches they catch up at their own pace; a squad more than
twelve tiles ahead of the commander uses the escort pace until he catches up.
The commander keeps that escort pace. All squads use ordinary collision rules. Front members respect that pace; only trailing members spend
catch-up credit. Queued intermediate destinations are passing points, and their
shared corridor remains available to the rear after the anchor turns.

After the owner's final waypoint or attack finishes, the General can pursue
contacts under automatic control again.

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
hashing: homes, destinations, membership, army waypoints, queued recruits, scan cursors, rotation
and retreat deadlines, plus the original stances and override reasons in 201
inline slots per army. Stance bookkeeping allocates nothing per decision.
No randomness, wall clock, network or client decisions.

At 20 Hz, admission runs every ten ticks. At most eight Generals are admitted,
rotating by entity ID. Each scans at most 1,024 spatial unit entries, 625 nearby
building-occupancy tiles and 25 buildings per indexed production kind. Each
issues at most 16 withdrawal commands and 21 formation or waypoint commands.
Recruitment makes at most one purchase per eligible building visited, bounded
by squad vacancies and the owner's resources and population.
Partial spatial censuses resume on the next admission and conservatively
refuse a fight. The global census bounds per admission pass are 8,192 unit
entries and 6,024 building or tile visits. Army membership is at most 200
soldiers plus the General.
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
