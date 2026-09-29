# Map polish checkpoint — 2026-09-29

The user stopped work for the day. Do not interpret this checkpoint as a request
to resume automatically. Continue only when asked.

## Location and Git

Worktree: `/home/anko/Work/projects/AoeWorld-final`.
Branch: `codex/fix-geographic-viewport`; target: `dev`.
PR: https://github.com/Anko59/AoeWorld/pull/74 .
Main polish commit: `34a07a5d05adea7dc3a55c1d4ff92205d875dde7`.
Follow-up commits on the same branch finish complete terrain atlas loading and
record final verification. Use the actual branch tip, not this older SHA.

Read root AGENTS.md, docs/agent-engineering.md and scoped instructions. Use
Dockerized Make targets. This worktree's shared Cargo cache needs:
`ROOT_MOUNTS='-v /home/anko/Work/projects:/home/anko/Work/projects'`.
For Git hooks, export that Make variable through MAKEFLAGS; never bypass hooks.

## What changed

See [render-polish.md](render-polish.md) for implementation details and limits.
Visible trees no longer have a 1,024-instance sampling cap. Picking, horse/tree
contact and rendering share the projected terrain mesh. Canvas allocations and
high-DPI backing resolution are bounded and presentation buffers reused.
Recipe 7 adds variable-density forests, irregular clearings, procedural trails,
forest-floor materials and an irregular central start glade. Recipes 3–6 remain
loadable. Broadleaf shadows use original paired art; other entities use fixed
alpha silhouettes.

All six reviewed terrain sources contain 100 flat 97×49 frames, with the same
x-major/reversed-y layout. Loading ten frames of dry grass produced severe seams
in the starting glade; the follow-up loads all 100 for every material. Height-
sorted atlas packing preserves semantic frame indices and fits the actual 746
selected frames into the existing 2048 atlas (last shelf ends at y=1969), without
quadrupling texture memory. Synthetic fixture rectangles were separated too.
Original assets and screenshots stay outside Git.

## Verification and unresolved work

At the main polish commit, hooks and `make preflight` passed: 702 native tests,
static checks, and synthetic smoke. `make test-wasm` passed 2 integration,
23 client and 55 rendering tests. `make browser-check` passed.

Two full `make test-e2e` runs exceeded the unchanged 300-second process deadline.
Individual dense-resource, memory and rendering checks passed before termination,
but neither run is a suite pass. Do not mark the PR ready or merge based on them.
The memory regression checks a 4K/DPR2 backing allocation once, then six dense
forest pan/resize cycles at ordinary sizes; assertions were retained. Software
Canvas repeatedly drawing unchanged frames was identified as a performance
problem. The follow-up includes an exact last-successful-frame cache to skip unchanged
rendering; scene replacement, movement, selection, depletion and grid changes
invalidate it. Validate it in the final browser suite before merging.

`make coverage` was terminated with exit 143 during its final long native
Sahara route/replay test. It is NOT a coverage pass. Earlier PR coverage was
very close to the 85% floor. Rerun the complete coverage gate and inspect CI;
never lower the floor. The main commit's GitHub run was 36505069289.

Remaining visual limitations: coarse terrain LOD stretches individual native
textures; transitions between different materials are still hard tile borders,
not AoE-style blending. Trails are procedural, not historical roads. The memory
checks measure WASM allocation growth, not browser/GPU RSS stability or readiness
for thousands of moving units. Those claims still require separate qualification.

## Playable local state

The release server at http://localhost:8081/ is container
`aoeworld-france-polish-preview`, with recipe-7 France active. It uses the release
binary and local private pack, not a production deployment. Static WASM files
are served from this worktree; refresh after rebuilding. The old server at
http://localhost:8080/ remains `aoeworld-france-10to1`, with the older map recipe.
Do not confuse the two when reviewing forests.

France package (10:1 compression, 70,000 tiles per axis):
`d27d08d423d95e737b50458f7b68c93a2eed1320a8bdb2aa1279cffe6c8f269a`.
Folder: `local-assets/france-10to1`. The prior recipe-5 package is preserved.
The recipe-7 horse starts around tile 34999.5,34999.5. Selection and a right-click
move were observed working in the new glade. Home centers on the primary unit.
Close agent preview tabs after inspection so they release the controller lease.
Restarting the server requires reactivating the saved map through the creator.

## Next session

1. Inspect latest commits, clean-tree status, PR74 checks and final wrap-up notes.
2. Run missing focused/full gates after any final follow-up; fix assertions or
   actual rendering cost rather than weakening gates.
3. Visually review all terrain materials, normal/minimum zoom, changing slopes,
   forest margins and trails with the local pack. The final complete-sheet build loaded the real pack successfully and its dry-
   grass clearing no longer showed the disconnected diamond seams. Broader
   material and zoom review remains useful.
4. Qualify browser RSS under a bounded long camera route before promising the
   crash is eliminated. Preserve the distinction from thousands-of-units RTS load.
5. Merge/clean PR74 only after required CI passes and review is satisfied.

## Wrap-up checks

The final texture/cache follow-up passed `make test-wasm` (2 integration,
26 client, 55 rendering tests) and `make browser-check`. The three added client
tests exercise successful-frame cache invalidation and retry behavior. The real
2048 atlas loaded successfully in the release game after all six terrain sets
were enabled. Final preflight and the normal pre-push hook results are recorded
in the closing PR description; full E2E and coverage remain unverified.
