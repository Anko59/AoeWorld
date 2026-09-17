# Troubleshooting

If bootstrap fails, check `docker info`, available disk space, and access to
the pinned image and crates registry. If the server cannot bind, inspect the
host port 8080. An unsupported WebGPU adapter should be reported as a browser
capability failure with the browser version and platform.

If a browser reports visible sprites but the canvas is blank, run
`make test-e2e` and inspect its full-page screenshot. The pinned browser gate
starts Chromium under Xvfb with GPU compositing and Vulkan SwiftShader; adapter
creation or draw counters alone do not prove that the canvas was presented.
See the [documented Chromium compositor failure and working flag set](https://github.com/visgl/luma.gl/issues/2874).
