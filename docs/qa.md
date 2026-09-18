# Exploratory QA

Start AoeWorld with `make dev`. This QA adapter targets the development
diagnostics at `/diagnostics.html`. In another terminal, run
`make qa-serve` and connect a compatible MCP client to its stdio. The Rust
server speaks MCP 2025-11-25. It exposes named local browser sessions,
accessible observations, visible button activation, scenario selection,
bounded canvas pointer/keyboard input, screenshots, state waits, diagnostics,
and evidence recording. The Playwright worker runs in the pinned browser image.
It does not expose arbitrary JavaScript, shell, filesystem browsing, or hidden
application mutation. The target is limited to localhost HTTP.

## Investigator instructions

Investigate as a player or tester who can see the browser. Do not inspect
source code or internal endpoints during the exploratory session. Open two
sessions when checking independent views. Use `observe` to read accessible
controls and status, and `screenshot` to save evidence under `reports/qa/`.
Record the exact visible steps, expected result, actual result, scenario, and
build identity for each finding. Reproduce before recording a finding.

The budgets are **fast: 15 minutes**, **full: 45 minutes**, and **extended:
120 minutes**. The default is fast. Complete these six journeys before a
`PASS` report:

1. Startup and visible diagnostics.
2. Camera movement and zoom.
3. Two browser sessions observing separate regions.
4. Disconnect and reconnect to a fresh state.
5. Invalid scenario or configuration feedback.
6. Unsupported browser capability feedback.

For the last two journeys, `open_session` accepts the bounded options
`configuration: "invalid-scenario"` and `capability: "webgpu-disabled"`.
The first loads a visible invalid URL scenario and the second disables WebGPU
only in that isolated browser context. Browser sessions run under Xvfb with
Vulkan SwiftShader so screenshots capture the WebGPU canvas. Each screenshot
returns the count of visible sprite-colored pixels in the canvas.

Use `record_journey` with an existing evidence file for each completed journey.
Use `record_finding` for any issue, then `finish` with `FINDINGS`. If access or
time prevents completion, finish with `BLOCKED`. `PASS` requires all journeys
with evidence and no findings. `make qa-validate` rechecks the saved
`reports/qa/session.json`. CI tests the MCP contract and validator; it does not
pretend that scripted browser tests are an exploratory agent session. No LLM
account or runtime-specific unattended launcher is required.
