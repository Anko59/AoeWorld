# Troubleshooting

If bootstrap fails, check `docker info`, available disk space, and access to
the pinned image and crates registry. If the server cannot bind, inspect the
host port 8080. An unsupported WebGPU adapter should be reported as a browser
capability failure with the browser version and platform.
