# Village economy

The authoritative rules live in
[`VILLAGE_RULES`](../crates/simulation/src/game_world/economy.rs).
It is the current gameplay table for starting resources, villagers, footprint,
population support, villager movement/carrying/gathering and training.
The scout retains the existing world's configured movement speed.

Map activation first runs the existing certified start search, then performs
bounded local placement for the Town Center and free perimeter spawn tiles.
The scout keeps the primary unit id. Placement preserves its found position
when the footprint allows it; a small certified clearing can instead hold the
Town Center while the scout starts on its perimeter. Footprints cannot cover
units, resources, water, other buildings or the map boundary. They participate
in map route planning, passability and diagonal collision.

Gather commands require an owned villager and a live immutable-map resource
node. A villager walks to a passable adjacent tile and earns integer gathering
credits from game ticks. Each complete resource unit uses the existing prepared
depletion transaction. Stored-map services persist that transaction before
committing it or adding to the carried load. A refused save discards the failed
attempt's credits and pauses retries for one game second.

Full loads and loads from exhausted nodes return to the nearest reachable owned
Town Center perimeter. Deposits change only the matching resource stockpile.
The job resumes the same node; after exhaustion it searches for a same-kind
node within the rules table's Euclidean radius around the exhausted node.
Without a replacement it becomes idle. Without an available drop-off it keeps
its carried load and retries on game time. Changing resource kind discards the
carried load; a plain Move cancels the job while keeping the load.

Train commands require an owned Town Center, enough food, a free population
slot and space in its bounded queue. Acceptance pays the food and reserves the
population slot; population used continues to count living units. Only the
front entry advances. A completed entry waits for a free passable perimeter
tile and does not pay again. New villagers use the shared entity allocator.
No houses, additional building commands, death, rally points or saved village
state are introduced here.

Gameplay protocol 2 is the only current wire. Gather and Train share Move's
bounded per-session/global command queue, sequencing and role checks. Snapshot
and tick subscription revisions also cover player stockpile/population and
building footprint/queue/progress. Per-session residents produce sparse changes;
failed delivery disconnects rather than losing a state delta. Resource revisions
continue through the independent bounded overlay feed. Reconnect/resync sends
complete current state. Wire collection and value bounds live in the protocol
crate; changing a gameplay value beyond those bounds also changes that current
wire contract and its golden fixtures in place.

The uniform compatibility start has stockpiles; immutable-map resource nodes
are available after activating a resource-bearing map. The existing client
receives the current wire, keeps its unit presentation and Move controls, and
shows readable command refusals. Villager/Town Center art and Gather/Train
interaction belong to the next client change; the AoE-style interface follows
separately.

Focused validation is `make duel-check HARNESS_DUEL_BASE=<merge base>` followed
by `make test-wasm` and `make test-e2e`. Economy tests cover exact tick rates,
all resource deposits, depletion/replacement, persistence refusal, training,
placement/collision, replay, wire fixtures/bounds and service lifecycle. The
headless test records its observed deposits and training in
`.cache/economy/headless.log` for the local narrated demonstration.
Synthetic evidence does not qualify finished RTS performance or hardware.
