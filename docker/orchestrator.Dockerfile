FROM docker:29.9.3-cli@sha256:2357639ad1ea7d0194134bf3292622fd4e562483966ed674e5d7c945b5f3b28d AS dockercli
FROM aoeworld/rust-tools:1.99.1-sccache0.19.0
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
RUN apt-get update && apt-get install -y --no-install-recommends gh=2.23.0+dfsg1-1 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
