# Firefox WebGPU on Linux

Use the core `rgba8unorm` canvas format for the game and diagnostics. Firefox
157 snap on Wayland with PRIME offload advertises `bgra8unorm` as preferred,
but that presentation path can leave the canvas black without validation
errors. A successful queue promise or canvas readback alone does not prove
that the browser displayed the frame.

On the reproduced Ubuntu hybrid-GPU setup (NVIDIA GTX 1660 Ti, driver
595.91.07, integrated AMD Renoir), the same shaders and render pass displayed
with RGBA. Both shader variants compiled without warnings. BGRA stayed black
on both adapters; on NVIDIA its queue/snapshot also stalled. Explicit
high-performance adapter selection uses the discrete GPU; PRIME's OpenGL
environment alone left the default WebGPU adapter on AMD.
Firefox's graphics failure log recorded `glImportSemaphoreFdEXT failed: 1282`
for NVIDIA BGRA and `Failed to get SourceSurface from DMABufSurface` for AMD
BGRA. These were visible in `about:support`, rather than page validation errors.

Firefox's [canvas configuration](https://github.com/mozilla-firefox/firefox/blob/main/dom/webgpu/CanvasContext.cpp)
uses a shared-texture swapchain only for formats accepted by its
[WebGPU client](https://github.com/mozilla-firefox/firefox/blob/main/gfx/wgpu_bindings/src/client.rs).
That list currently contains BGRA alone. RGBA selects Firefox's readback
presentation path, avoiding the broken shared-texture handoff while retaining
WebGPU shader rendering. The browser's copy adds cost; this workaround does
not establish hardware frame cadence.
An RGBA run can log `WebGPU: disabling SharedTexture swapchain: canvas
configuration format not supported` in the graphics failure log. That refers
to the shared-texture path; the RGBA WebGPU canvas still renders successfully.

Keep the initialization, queue and blank-frame fallback checks. A browser
without a usable adapter must still select WebGL2, then Canvas if needed.

## Real-machine check

Run this after changing canvas configuration, startup presentation or shaders.
Use a fresh profile and the checkout's own server. Never use the owner's
Firefox profile or an occupied server port.
Select an imported local pack with `AOE_ASSET_PACK=local-assets/packs/<pack-hash>`
when starting the server so this check uses the real village art.

```sh
make dev DEV_PORT=8302
AOE_FIREFOX_PROFILE=$(mktemp -d "$PWD/.cache/firefox-webgpu.XXXXXX")
printf '%s\n' 'user_pref("dom.webgpu.enabled", true);' \
  'user_pref("devtools.console.stdout.content", true);' \
  > "$AOE_FIREFOX_PROFILE/user.js"
WAYLAND_DISPLAY=wayland-0 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/1000 \
  __NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia \
  MOZ_LOG=WebGPU:5,console:5 /usr/bin/firefox --no-remote --new-instance \
  --profile "$AOE_FIREFOX_PROFILE" 'http://localhost:8302/?play=1'
```

Capture the console and a page screenshot through Marionette or Playwright.
Store screenshots and raw logs only under `.cache/`. Verify all of:

- The village and terrain are visible in the screenshot, including player units.
- `document.querySelector('#playground').dataset.renderer` is `webgpu`.
  The renderer attribute belongs to `#playground`, not the `#scene` canvas.
- The adapter log reports `canvas Rgba8Unorm`; there are no WebGPU validation,
  lost-device, timeout or fallback messages.
- Camera movement, returning Home and resizing continue to display the world.

Record the exact browser, driver and requested adapter. Firefox's compositor
GPU and the default WebGPU adapter listed in `about:support` can differ from
the game's high-performance adapter. Synthetic shader pixels do not qualify
finished-game performance or hardware cadence.

Close the throwaway Firefox instance, then stop this checkout's server:

```sh
make down DEV_PORT=8302
```

## Automated regression

```sh
make test-e2e AOE_E2E_SPECS='tests/rendering/recovery/format.play.spec.ts tests/rendering/recovery/firefox.play.spec.ts tests/rendering/recovery/device.play.spec.ts'
```

The format regression advertises BGRA but suppresses its submissions, requiring
real RGBA WebGPU pixels without fallback. The Firefox project requires WebGPU
when its adapter probe succeeds; otherwise it checks playable fallback. It
attaches the browser version, adapter availability, console and screenshot.
A CI pass without a Firefox adapter does not replace the headed check above.
