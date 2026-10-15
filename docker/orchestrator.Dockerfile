FROM docker:29.9.0-cli@sha256:1a4c7cb63513f349bdad01fcc6e0f3f2f67d37b9da86f14dc0d4a0942eecda00 AS dockercli
FROM aoeworld/rust-tools:1.93.1
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
RUN apt-get update && apt-get install -y --no-install-recommends gh=2.23.0+dfsg1-1 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
