# Live game administration

Set `AOE_MATCH_TOKEN` when starting the server (including `make dev`). Every
endpoint below requires the existing `x-aoeworld-admin-token` header; an absent,
wrong, or unconfigured secret returns HTTP 403. Player/AI seat tokens do not
permit administration. These endpoints observe or change the currently active
match, including a match started from the skirmish menu. They never return seat
tokens. Use a private local connection when sending the admin secret.

For example, use your own dev port and obtain the secret through the normal
credential setup described in [credentials](credentials.md). In a shell with
that secret available as `AOE_MATCH_TOKEN`:

```sh
export AOE_ADMIN_URL=http://127.0.0.1:8186
curl -fsS -H "x-aoeworld-admin-token: $AOE_MATCH_TOKEN" "$AOE_ADMIN_URL/admin/match"
curl -fsS -H "x-aoeworld-admin-token: $AOE_MATCH_TOKEN" "$AOE_ADMIN_URL/admin/match/players/1?offset=0&limit=100"
curl -fsS -H "x-aoeworld-admin-token: $AOE_MATCH_TOKEN" "$AOE_ADMIN_URL/admin/perf"
curl -fsS -X POST -H 'content-type: application/json' \
  -H "x-aoeworld-admin-token: $AOE_MATCH_TOKEN" \
  --data '{"disabled":true}' "$AOE_ADMIN_URL/admin/fog"
```

`POST /admin/fog` accepts exactly `{ "disabled": bool }` and returns the same
object. Set `disabled` to `false` to restore fog. Every connected session,
including AI sessions, receives the new fog view on its next tick. Ordinary
sight and unexplored knowledge resume when fog is restored.

Fog-off sends all entities within each viewer's bounded camera subscription,
plus that viewer's own units and buildings. Panning updates that set. It does
not copy the whole world's enemy buildings into every session. Buildings use
one shared server frame, and existing entity/message limits and streamed
building delivery still apply. Fog payloads stay bounded by the subscription
and fixed overview size, independent of map area; disabling fog allocates no
map-sized visibility grid. The minimap uses its existing schematic overview
rather than downloading the entire map.

`GET /admin/match` returns:

- `tick`, `fog_disabled`, and `players` sorted by player id.
- Each player's `id`, `kind` (`human` or `ai`, from its seat), `status` (`alive`
  if any living unit or building remains, otherwise `eliminated`), and `age`.
- `population` (living units), counts by kind in `units` and
  `buildings` (including foundations, excluding corpses), and `resources`
  (`food`, `wood`, `gold`, `stone`).
- `town_centers`, and `army` with `size` and `centroid` (`null` for no army).
  Army includes scouts and military units; villagers and fishing ships are
  excluded. All positions use game tiles, including fractional coordinates.
- `activity`, the most common living unit activity: `gathering`, `building`,
  `idle`, `attacking`, or `marching`. Gathering includes farming, fishing,
  hunting, shepherding and returning loads. Attack orders include their
  approach march. Other moving/planning units count as marching. Ties follow
  that listed order; an empty player is idle.
- Lifetime combat `kills` and `losses` (units and buildings). Counters survive
  corpse cleanup; wildlife targets and cancellation are excluded. Wildlife
  attacks on player units count as hits/losses without player kill credit. Projectile kills
  remain attributed to the firing player even if the attacker has died.
- `combat_per_minute`: `melee_hits`, `ranged_hits`, `deaths`, counted on actual
  impacts on player units/buildings over the trailing sixty **simulation**
  seconds. Launches, absent targets and damage to wildlife do not count.

Summaries are computed on request with one unit/building scan. Combat adds
small counters only when damage resolves, with bounded recent tick buckets;
there is no per-tick summary scan or retained entity history.

`GET /admin/match/players/{id}` returns `id`, `tick`, `offset`, `limit`, `total`,
`next_offset` (`null` at the end), and an `entities` array. Each record has `id`,
`category` (`unit` or `building`), `kind`, tile `position`, and `task` (unit task
or building construction state). Only living entities are included. Default
limit is 100; valid limits are 1–500, otherwise HTTP 400. Unknown players return
404. The limit applies to both categories together. Ordering is unit storage
order followed by building id order; entity changes can move page boundaries
between requests, so compare `tick` and refresh if you need a coherent snapshot.
Only the requested page's records are serialized.

`GET /admin/perf` summarizes the last sixty wall-clock seconds, at most 1,200
completed ticks. See [performance](performance.md) for units, window semantics,
phase overlap, memory limits and the client overlay. Existing `GET /match/stats`
remains available for raw timing rows using the same secret.
