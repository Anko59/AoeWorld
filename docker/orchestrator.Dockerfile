FROM docker:29.9.1-cli@sha256:6fae2214c3121a28d5aa639d09032f61310b7c9c75ffbad1025f261a1b621800 AS dockercli
FROM aoeworld/rust-tools:1.99.1
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
RUN apt-get update && apt-get install -y --no-install-recommends gh=2.23.0+dfsg1-1 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
