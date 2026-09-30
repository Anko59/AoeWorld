# Map polish handoff — 2026-09-30

The user explicitly requested that all changes be pushed, a handoff written,
and work stopped. The persisted session goal is paused. **Do not resume unless
asked.** Do not merge PR #74 without authorization.

## Revision and location

Implementation revision: `aa050bbc88c94e94bb73e1bd20965e22b3909708`.
This document is committed separately immediately afterward; use the actual
remote branch tip for the complete handoff. Branch:
`codex/fix-geographic-viewport`, target `dev`.
PR: https://github.com/Anko59/AoeWorld/pull/74 .
Worktree: `/home/anko/Work/projects/AoeWorld-final`.
The separate `/home/anko/Work/projects/AoeWorld` checkout is not this branch.
The implementation commit passed its normal pre-commit hook. The closing
message records the final pushed tip and clean-tree status.

Read root/scoped AGENTS.md, the engineering guide and testing documentation.
Dockerized commands in this checkout need:

```sh
make GID=117 ROOT_MOUNTS='-v /home/anko/Work/projects:/home/anko/Work/projects' TARGET
```

For Git hooks, inherit the same settings; never bypass hooks:

```sh
export MAKEFLAGS='GID=117 ROOT_MOUNTS=-v\ /home/anko/Work/projects:/home/anko/Work/projects'
```

## Completed implementation

See [render-polish.md](render-polish.md) for the full design and limits.
The colleague's recipe-7 forests, clearings, paths, paired tree shadows,
minimum-zoom canopy retention and shared terrain contact remain intact.

- Coarse terrain is subdivided into bounded art patches while preserving its
  original planes and contact heights; grass is no longer stretched into huge
  diamonds at ordinary minimum zoom. The 24,576-triangle budget is unchanged.
- Shared world-keyed vertex materials crossfade native grass/dirt/sand/rock
  textures on both GPU and Canvas. Water and discontinuous cliffs are excluded.
  Pixel tests compare real GPU output with Canvas, not just shader strings.
- Material ownership and contact buckets share a compact coordinate index.
  Keys are stored once; contact values retain the old first-triangle order.
  Tests cover deliberate collisions, negative/extreme keys and the maximum
  73,728 vertex / 98,304 contact-key budgets. Geometric growth can temporarily
  use more storage than an ordered map; browser tests qualify the combined RSS.
- One composed stable painter sort preserves depth, kind and object-ID ordering,
  including shadow/body and exact resource/unit-ID collision ties, without the
  redundant object sort. Original two-sort behavior is compared directly.
- The asset loader packs by indices instead of cloning frame records. Its
  lightweight DTO preserves required fields, types and duplicate-field errors
  without retaining unused metadata strings. Raw JSON is freed before decoding.
  Bounded startup insertion sorting replaces generic sorting code for the
  reviewed 746-frame catalogue; malformed counts fail before quadratic work.
- Fixed ring coordinates use exact precomputed pinned-WASM float bits. Wheel-only
  exponential zoom uses browser math; simulation/replay math is unchanged.
- Replaced GPU depth textures and instance buffers are explicitly destroyed.
  The 64 MiB instance, 512-chunk/128 MiB cache and backing-resolution caps remain.
- Added sampled Chromium process RSS checks and a required-input real-source
  camera-route target. The replay browser test now waits for actual asynchronous
  WASM initialization before invoking the unchanged replay comparison.

No original asset pixels, local screenshots, baselines or toolchain changes
were committed. A separate read-only review found no concrete defects.

## Verification completed before the stop request

Commands below used the Docker settings above on the exact implementation
contents, initially at dirty base `802f1c325971de9df86fdc229a03d9d41ceccb52`.
Local reports therefore name that dirty base; they are not clean-commit CI proof.

| Command | Result |
| --- | --- |
| `make hooks-install hooks-check` | PASS |
| `make preflight browser-check` | PASS; 703 native tests, formatting/lint/policy/structure/docs, synthetic smoke, TypeScript/lint/Prettier |
| `make test-wasm` | PASS: 2 integration, 33 client, 67 rendering tests |
| `make test-e2e` | PASS: 44 passed, 8 external-input checks skipped, 3.2 minutes |
| `make test-memory-source` with active France URL/hash | PASS on WebGPU, forced Canvas and default Chromium, 2.1 minutes |
| `make perf-ci` | PASS; all native instruction/allocation comparisons and three synthetic workloads |
| `make perf-pressure` | PASS |
| `git diff --check` | PASS |

**Optimized gzip WASM: 227,504 bytes.** Unchanged baseline 217,240 bytes,
unchanged +5% cap 228,102 bytes: 598 bytes of headroom. Never increase the gate
or baseline to conceal a regression. The actual catalogue startup-sort probe
measured 0.110 ms versus standard sorting's 0.040 ms; this is only informational
sorting timing, not page-load or dedicated-hardware qualification.

The final real-source route visited **1,119 unique chunks per backend**, reached
512 resident chunks and returned Home. All assertions retained: sampled summed
VmRSS <=1 GiB, final-four-sample spread <=64 MiB, no browser exceptions.

| Backend | Peak summed VmRSS (MiB) | Final-four spread (MiB) |
| --- | ---: | ---: |
| WebGPU | 883.50 | 10.87 |
| Canvas | 783.17 | 5.34 |
| Browser defaults | 781.11 | 40.19 |

Ignored numeric evidence is preserved locally in
`reports/qa/source-memory/{webgpu,canvas,browser-defaults}-final-1105.json`.
The ordinary E2E run does not qualify external-input checks that it skips; the
real-source memory check was run separately with required inputs.

Earlier failures were not counted as passes: an asynchronous WASM replay-test
startup race was fixed; a source run's simultaneous `ERR_NETWORK_CHANGED`
fetch failures disappeared when concurrent Docker bridge creation stopped.
Serialize browser qualification with other Docker jobs and all `web/pkg`
writers. Do not weaken assertions or deadlines to accommodate contention.

## Explicitly unfinished / next session

- **Final complete `make coverage` and `make fuzz-smoke` were not run** before
  the user stopped work. Coverage thresholds remain 85% overall / 90% groups.
- The newly pushed revision's required GitHub checks have not been observed.
  Check PR74 CI before declaring it ready. The older run `36505955719` passed
  static/native coverage/browser/fuzz but failed WASM size; that is not evidence
  for this implementation revision. Push runs the normal preflight hook.
- The earlier local minimum-zoom image was reviewed: fine grass grain, smoother
  forest margins, full canopy and visible horse. A fresh broader normal/minimum
  zoom and sloped moving-horse visual review after the final optimizations was
  not completed. Rendering/contact/order equivalence tests passed instead.
- No indefinite soak, thousands of moving units or dedicated GPU/hardware
  qualification. VmRSS counts shared pages repeatedly and excludes host GPU
  allocations. The 24-region source route has one stationary unit.
- Blending is procedural splatting, not authored AoE transition masks, and may
  soften narrow tracks. Extreme viewports can still stretch textures after the
  unchanged geometry budget is exhausted. Paths are procedural, not historical.

When explicitly asked to resume: inspect the pushed tip and CI, run the missing
coverage/fuzz gates, perform remaining visual qualification, and keep the tree
clean. Do not start a new implementation or change performance baselines first.

## Private playable source state

France recipe-7 hash:
`d27d08d423d95e737b50458f7b68c93a2eed1320a8bdb2aa1279cffe6c8f269a`.
Directory: `local-assets/france-10to1`, 70,000 tiles/axis at 10:1 compression;
horse near 34999.5,34999.5. Private assets use ignored `local-assets/packs`.
The local preview was http://127.0.0.1:8081/ in container
`aoeworld-france-polish-preview`. It is not a deployment and may be stopped
when the session ends. Restart/activate the saved map before source qualification.

```sh
AOE_POLISH_SOURCE_URL=http://127.0.0.1:8081 \
AOE_POLISH_SOURCE_HASH=d27d08d423d95e737b50458f7b68c93a2eed1320a8bdb2aa1279cffe6c8f269a \
make GID=117 ROOT_MOUNTS='-v /home/anko/Work/projects:/home/anko/Work/projects' test-memory-source
```
