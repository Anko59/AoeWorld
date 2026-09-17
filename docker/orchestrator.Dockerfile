FROM docker:29.1.3-cli@sha256:e0b6ff45302985d99b9685b2a2fec50c3379bd47067493968f51da901dc53ad5 AS dockercli
FROM aoeworld/rust-tools:1.93.1
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
WORKDIR /workspace
