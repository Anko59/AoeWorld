# Management hierarchy: specification

Status: agreed with the owner on 2026-11-25, track by track. Owner rule for every track: "Management units control
an area and the layer units under them. They develop their land by creating units from the layer under them at
strategic locations in their area. The very top layer can only be created by players." Build it as written, and
change this file in the same pull request when a decision changes. Mechanics not settled here (marked **LATER**) wait
for the owner; do not invent them.

Why: the game targets 2M units on real country-size maps with up to 99 AI realms on one 12-core, 14 GB machine
([scale roadmap](../roadmap/scale.md)). No manager can look at every unit every tick, and no CPU can simulate every
unit nobody watches. The hierarchy solves both: a strict chain of offices where each office commands only the level
below it and thinks at its own pace, and where a territory nobody watches is played by its office instead of by its
units.

Owner rules this spec follows:

- "It should look right to a human playing." Unwatched play uses heuristics and never chases exactitude (11-24).
- "Solve, not cap": no counts or budgets that hide work (11-23). Cadences per office are intervals, not caps.
- The player can always command any office or any unit directly, and the player's order wins.
- No supply or attrition: armies fight anywhere at full strength (owner 11-25).
- AIs move from Python to Rust (11-24); the Python SDK stays as a thin client over the same intents.

## 1. Vocabulary

| term | meaning |
| --- | --- |
| Province | A connected region of the map, generated from geography (section 2). The unit of territory, orders and reports. |
| Track | A line of command for one kind of work: Army, Economy, Navy, Church, Intelligence, Works, Expansion. |
| Rank | A level of a track's ladder, with a span (how many it leads) and a cadence (how often it thinks). |
| Office | A slot of a given rank and track, bound to a territory (a site, a province or a group of provinces). It outlives its holder. |
| Holder | The unit that fills an office (a hero, a villager, a ship, a monk). Losing the holder does not delete the office. |
| Intent | An order from an office (or the player) to an office one rank below: what to achieve, where, with what priority. |
| Report | A summary from an office to the office above: state, needs, threats, progress on intents. |
| Watched | A tile inside any human player's view plus a margin; unwatched otherwise (section 6). |
| Detail mode / office mode | How a territory is simulated: units one by one (watched) or by its office's heuristics (unwatched). |

## 2. Provinces

- Generated once per map package, from the same geodata that builds the map: watershed basins first, split by
  ridges and coasts, merged or cut to about 100-300 tiles across (area about 10k-90k tiles). Islands and coastal
  strips are their own provinces. Generation is deterministic and stored in the package with stable ids.
- Each province stores: tile set (as chunk-run encoding), centre and seat site (best settlement tile), adjacency with
  crossing points (bridges, fords, passes, harbours), resource totals by kind, terrain mix, travel time to each
  neighbour along roads and paths.
- Ownership is dynamic: a province belongs to the realm whose offices control its seat; contested when two realms
  hold offices in it. Control is what reports and the far view show.
- The far view is the province map: borders, owners, offices, armies as tokens on roads, reports as badges.
- Procedural maps get provinces from the same generator over their synthetic terrain.

## 3. Offices and ladders

Spans are the number of subordinates an office leads; each office leads only the rank directly below.
Cadence is how often an office runs its decision step (simulation runs at 10 Hz).

**Army** (owner's ladder, 10-31)

| rank | leads | territory | cadence |
| --- | --- | --- | --- |
| Sergeant | about 20 soldiers | a squad's position | every tick |
| Centurion | about 5 sergeants (100) | a battlefield sector | 1 s |
| Lieutenant | about 10 centurions (1,000) | a province | 5 s |
| General | about 10 lieutenants (10,000) | a front (several provinces) | 30 s |

**Economy** (owner's ladder, 10-31)

| rank | leads | territory | cadence |
| --- | --- | --- | --- |
| Leader | about 20 villagers | a work site (wood line, mine, farm block) | every tick |
| Prefect | about 5 leaders (100) | a village | 1 s |
| Steward | about 10 prefects (1,000) | a province | 5 s |
| Consul | about 10 stewards (10,000) | a region of provinces | 30 s |

Today's in-game Steward (100 villagers, one Town Center) becomes the Prefect office, and "Steward" names the province
rank, following the owner's ladder. Today's General becomes the General office. Today's Vassal splits (owner 11-25):
building, roads and walls go to the Works Castellan, the territory economy to the province Steward.

**Navy**: Captain (one ship, every tick), Commodore (about 10 ships, 1 s), Admiral (a sea zone, 5 s). Orders:
- Control a zone: patrol, attack any enemy ship that enters, repair at a dock when damaged. A blockade is control of
  a small zone around an enemy port.
- Ferry an army: pick a landing beach, bring transports, load, sail with escorts, unload. The General keeps command
  of its army during the crossing; the Admiral only moves the ships.
- Scout the sea: keep every zone recently seen with light ships; report fleets, docks and coasts with their age. The
  Navy scouts the sea, Intelligence the land; both feed the threat map.
- Fishing is not Navy: fishing ships belong to the coastal economy offices.

**Church**: Abbot (a monastery and its monks, 1 s), Bishop (a province's monasteries, 5 s), Archbishop (realm, 30 s).
Orders:
- Heal: monks follow an army or stay near a town and heal wounded units in range.
- Lend monks: a General asks its province's Bishop for healers; the Bishop sends monks to follow that army.
- Convert: monks with an army convert enemies in range, expensive units first (knights, siege, elephants).
- Relics: find (from Intelligence), carry home, garrison in a monastery, defend it. Relics are a new game feature
  (gameplay roadmap).
- The Archbishop sets the realm's balance between healing, converting and relics. Monasteries are requested from
  Works.

**Intelligence**: Outrider (scouts of one province, every tick), Ranger captain (a border's watch, 1 s), Spymaster
(realm, 5 s). Orders:
- Scout a province: scout cavalry sweep provinces not seen recently, avoid fights, flee when attacked.
- Watch a border: scouts posted at crossings, passes and roads report every army that passes.
- Expand vision: the Spymaster chooses outposts at crossings, hills and road junctions; Works builds them. Outposts
  see twice as far as today (game rule change, gameplay roadmap).
- Threat map: every enemy army, town and fleet seen, with size, position and age; Army offices and the far view
  read it.
- Find relics, resources and free land for Expansion. Intelligence only knows what its units and outposts saw.

**Works and defence**: Master builder (one site, every tick), Castellan (a province, 1 s), Warden of the March (a
border of provinces, 5 s). Orders:
- Build on request: Stewards (Mills, houses, farms), Church (monasteries), Intelligence (outposts), Army (castles,
  workshops); the Castellan opens Master builder sites with builders from the province's economy.
- Roads and bridges linking a province's villages, mines and borders (today's Vassal road work).
- Fortify a border: the Warden picks wall lines and tower spots at chokepoints; Castellans build them.
- Repair damaged defences first, then other buildings.

**Expansion**: Pioneer (a new settlement, 1 s), Margrave (a frontier province, 5 s), Viceroy (conquered or settled
provinces, 30 s). Orders:
- Settle a province: a Pioneer picks the town site in free land Intelligence reported, escorts villagers, builds a
  Town Center, then hands the town to a new Prefect under that province's Steward.
- Develop a frontier: a Margrave requests houses, a market, defences and villagers until the province pays for itself.
- Govern conquests: a Viceroy replaces lost Town Centers, keeps economy offices running and asks the Army to hold the
  border.

**Ruler**: the player, or the realm's AI. Only a player (or the AI ruler) creates top-layer offices; it issues
intents to them.

Rules for every track (owner 11-25):

- **Creation from above.** Every office develops its area by creating offices of the rank below at strategic places
  in it (a province Steward founds Prefects where villages and work sites belong; a Lieutenant forms Centurions; a
  Castellan opens Master builder sites) and staffs them. Only the top layer is created by players. Nobody has to
  place lower offices by hand.
- **Capacity.** Each office has a design capacity, as today's Steward has 100 villagers; offices do not grow, split
  or merge on their own. Any growth mechanic is **LATER**.
- **Succession.** The lowest rank of a track has no specific asset and is replaced automatically. Higher offices go
  vacant when their holder dies; the office above (or the player) appoints a new holder.
- **Influence.** An office acts only inside its area; a vacant office's branch keeps its last orders, which makes
  killing commanders matter.
- **Player orders.** The player can direct any office at any rank. A direct order to a unit detaches it from its
  office until released or done; an order to an office replaces that office's current intent.
- **LLM council.** An LLM council may direct only the top layer, never a lower office.

## 4. Intents and reports

Intents name a goal, a place (province, site or tile), a priority and an optional deadline. They never name
individual units below the receiver's direct subordinates.

| track | intents |
| --- | --- |
| Army | secure, hold line, raid, siege, escort, pursue, retreat to, garrison |
| Economy | produce (kind, per minute), stockpile (kind, amount), settle workers, supply (send to province) |
| Navy | control zone (a blockade is a small zone at a port), ferry (army, from, to), scout sea |
| Church | heal (army office), convert, hold relic, build monastery |
| Intelligence | scout province, watch border, track army |
| Works | build (kind, site), fortify province, road (from, to), bridge (crossing), repair |
| Expansion | settle province, annex, hold frontier, abandon |

Reports, upward every cadence of the reporting office: strength (units by kind, health), work rate, stock, needs
(units, resources, builders), threats seen (with intel age), progress on each intent, and an alert flag that wakes
the office above before its next cadence (attack, holder killed, intent impossible).

Offices across tracks coordinate only through their common superior, except three standing links: Church lends
chaplains to Army; Works takes build requests from Economy, Army and Expansion; Intelligence publishes threat maps
every track can read.

## 5. The office engine

- One generic engine runs every track: office records, the rank cadence scheduler (each office on its own phase,
  spread over its interval), intent and report queues, creation of lower offices, succession. Tracks plug in decision functions:
  `decide(office view, intents, reports) -> intents for subordinates + actions`.
- Decisions run in parallel: every office due this tick decides on a worker against a read-only world view; actions
  are applied on the main thread in office-id order, so results do not depend on thread count.
- Leaf offices (Sergeant, Leader, Captain, Outrider, Master builder) are where units get orders; they reuse today's
  unit orders (attack-move, gather, build, train) and today's Steward and General logic as their first decide
  functions.
- Cost target: an office's decision step costs about the same whatever the realm size; total work grows with units,
  not with offices times units.

## 6. Watched and unwatched play

- **Watched set**: every chunk inside a human player's view, plus a margin of one chunk, plus every province where a
  human-owned unit fights. Computed every tick from views; hysteresis of a few seconds avoids flicker.
- **Detail mode** (watched): units simulate one by one, as today.
- **Office mode** (unwatched): the territory's offices play their subtree as aggregates:
  - Units are grouped by office into counts by kind with pooled health and experience, positioned at road nodes,
    sites or squad anchors, not per tile.
  - Economy: work rates from workers, site richness and travel time; stock changes per office cadence; depletion
    applied to resource totals per site.
  - Movement: along the province road and path graph, at the slowest member's speed, with crossing points as costs.
  - Combat between aggregates: a Lanchester-style exchange by unit kind with the game's own attack and armour
    values, terrain and walls as modifiers, resolved every office cadence.
  - Production, research and building: queue times as in detail mode.
- **Transitions**: entering the watched set materialises the aggregate into units at plausible tiles (formations,
  work sites, along roads), deterministically from the office state and a seed; leaving folds units back. A unit the
  human has seen keeps its identity across folds (id, health, experience).
- **Acceptance ("looks right")**: no visible teleports at the view edge; a watched and an unwatched copy of the same
  fight or economy end within the spread a human would accept (for example kills within about 20%, stock within about
  15% over five minutes), measured as plausibility tests on the 5x and 10x benches, not as equality.

## 7. AIs

- Every realm's AI is a Rust ruler that issues intents to its top offices; it runs in process on the worker pool like
  any other office decision. Strategy lives at the ruler, Consul, General and Viceroy levels; everything below is the
  shared office engine, identical for AI and human realms.
- The Python SDK keeps working as a client that sends intents and reads reports; it is no longer the default AI.
- A later LLM counsel is just another ruler implementation over the same intent language.

## 8. Player experience

- The far view is the province map with office tokens; the near view is today's game.
- Selecting an office shows its holder, territory, current intents, last report and subordinates; the player edits
  intents there. Office cards replace today's separate Steward, Vassal and General panels.
- Reports reach the player as a feed filtered by rank; alerts (attack, commander killed, intent impossible) are
  notifications with a jump-to.
- Delegation: the player can give any top office to the realm AI and take it back at any time.

## 9. Scale and performance targets

- 2M units, 99 realms, real time at 10 Hz on the 12-core machine, with any one human's view in detail mode.
- Memory: about 4 KB per unit on average; units in office mode cost a share of their aggregate, not a full record.
- Gates: the real-game CI workload ([performance](../performance.md)) on the critical path, plus local replays of the
  owner's save (R1) and the 5x and 10x benches.

## 10. Build order

1. Provinces: generator, package format, far-view province map (no behaviour change).
2. Office engine with Army and Economy on today's logic: General becomes an office; Steward becomes the Prefect; the
   Vassal splits into Works (Castellan) and Steward rank. Parity with today's behaviour on R1.
3. Remaining tracks: Works, Navy, Church, Intelligence, Expansion.
4. Rust rulers replace the Python AIs.
5. Office mode for unwatched territory, transitions, plausibility tests on the 5x and 10x benches.
6. Office cards and report feed in the UI.

Each step ships on its own and is judged on real games (R1 and the benches), never on procedural kernels alone.

## 11. Not in this spec yet

- **LATER: office growth.** Offices keep a design capacity; no growth mechanic until one is needed and agreed.
- **LATER: trade and diplomacy tracks**, when the owner raises them.
