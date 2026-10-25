FROM docker:29.9.2-cli@sha256:5e6baf67325a79f64ed3bed6863954d5e05d0f3d4e57cbdb14809346814bbfdb AS dockercli
FROM aoeworld/rust-tools:1.99.1
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
RUN apt-get update && apt-get install -y --no-install-recommends gh=2.23.0+dfsg1-1 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
