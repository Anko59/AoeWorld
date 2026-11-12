FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c
RUN apt-get update && apt-get install -y --no-install-recommends \
    gdal-bin=3.6.2+dfsg-1+b2 libgdal-dev=3.6.2+dfsg-1+b2 \
    libclang-dev=1:14.0-55.7~deb12u1 \
    libproj-dev=9.1.1-1+b1 proj-bin=9.1.1-1+b1 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown && rustup component add clippy rustfmt
RUN cargo install wasm-bindgen-cli --version 0.2.128 --locked
RUN curl --fail --location --silent --show-error https://get.nexte.st/0.9.144/linux --output /tmp/cargo-nextest.tar.gz \
    && echo '8a4f726272b0a1c499bd87ca3978bfbb1a8c20bb08ccf075b9996e2081bd1e1e  /tmp/cargo-nextest.tar.gz' | sha256sum --check \
    && tar -xzf /tmp/cargo-nextest.tar.gz -C /usr/local/cargo/bin cargo-nextest \
    && rm /tmp/cargo-nextest.tar.gz
RUN curl --fail --location --silent --show-error https://github.com/mozilla/sccache/releases/download/v0.19.0/sccache-v0.19.0-x86_64-unknown-linux-musl.tar.gz --output /tmp/sccache.tar.gz \
    && echo 'aa73c7270c997894038c0d8f0d297865f57569e1687883322bc431713d4bb9dd  /tmp/sccache.tar.gz' | sha256sum --check \
    && tar -xzf /tmp/sccache.tar.gz -C /tmp --strip-components=1 \
    && install /tmp/sccache /usr/local/bin/sccache \
    && rm /tmp/sccache /tmp/sccache.tar.gz
COPY crates/harness/src/compile_cache/mod.rs /tmp/aoe-compile-cache.rs
RUN rustc --edition=2024 -O /tmp/aoe-compile-cache.rs -o /usr/local/bin/aoe-compile-cache \
    && rm /tmp/aoe-compile-cache.rs
WORKDIR /workspace
