FROM docker:29.10.0-cli@sha256:34507eb5069d8be82c95dd3b6987d4250fd5fee49b82e2509b7d7a7030740d6d AS dockercli
FROM aoeworld/rust-tools:1.99.1-sccache0.19.0
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
RUN apt-get update && apt-get install -y --no-install-recommends gh=2.23.0+dfsg1-1 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
