FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c
RUN apt-get update && apt-get install -y --no-install-recommends \
    gdal-bin=3.6.2+dfsg-1+b2 libgdal-dev=3.6.2+dfsg-1+b2 \
    libclang-dev=1:14.0-55.7~deb12u1 \
    libproj-dev=9.1.1-1+b1 proj-bin=9.1.1-1+b1 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown && rustup component add clippy rustfmt
# Rouille uses tiny_http's connection pool to serve the runner's JS and WASM.
# Keep the local pool fix until the CLI's pinned dependencies include it.
COPY docker/wasm-bindgen/ /tmp/wasm-bindgen-task-pool/
RUN curl --fail --location --silent --show-error \
      https://static.crates.io/crates/tiny_http/tiny_http-0.12.0.crate \
      --output /tmp/tiny_http.crate \
    && echo '389915df6413a2e74fb181895f933386023c71110878cd0825588928e64cdc82  /tmp/tiny_http.crate' | sha256sum --check \
    && tar -xzf /tmp/tiny_http.crate -C /tmp \
    && cp /tmp/wasm-bindgen-task-pool/task_pool_tests.rs /tmp/tiny_http-0.12.0/src/util/ \
    && printf '\n#[cfg(test)]\nmod task_pool_tests;\n' >> /tmp/tiny_http-0.12.0/src/util/task_pool.rs \
    && curl --fail --location --silent --show-error \
      https://static.crates.io/crates/wasm-bindgen-cli/wasm-bindgen-cli-0.2.128.crate \
      --output /tmp/wasm-bindgen-cli.crate \
    && echo '2e29140c04e81832902b70e5d37b9ce1bfa811c8fe4563bea08f76df8388cf20  /tmp/wasm-bindgen-cli.crate' | sha256sum --check \
    && tar -xzf /tmp/wasm-bindgen-cli.crate -C /tmp \
    && patch --batch --fuzz=0 --directory=/tmp -p1 --input=/tmp/wasm-bindgen-task-pool/tiny-http-task-pool.patch \
    && printf '\n[patch.crates-io]\ntiny_http = { path = "../tiny_http-0.12.0" }\n' >> /tmp/wasm-bindgen-cli-0.2.128/Cargo.toml \
    && rustc --edition=2021 --test /tmp/tiny_http-0.12.0/src/util/task_pool.rs -o /tmp/wasm-bindgen-task-pool/regression \
    && /tmp/wasm-bindgen-task-pool/regression \
    && cargo install --path /tmp/wasm-bindgen-cli-0.2.128 --locked \
    && rm -rf /tmp/tiny_http.crate /tmp/tiny_http-0.12.0 /tmp/wasm-bindgen-cli.crate /tmp/wasm-bindgen-cli-0.2.128 /tmp/wasm-bindgen-task-pool
# x86-64 in CI and on Linux hosts; arm64 in the Colima VM of an Apple Silicon Mac.
RUN case "$(uname -m)" in \
      x86_64) platform=linux sum=8a4f726272b0a1c499bd87ca3978bfbb1a8c20bb08ccf075b9996e2081bd1e1e ;; \
      aarch64) platform=linux-arm sum=7fecfd431b810c05c589d800524286b8f80ce2fe9fb1fef05caf16d095cea407 ;; \
      *) echo "cargo-nextest: no pinned build for $(uname -m)" >&2; exit 1 ;; \
    esac \
    && curl --fail --location --silent --show-error "https://get.nexte.st/0.9.144/$platform" --output /tmp/cargo-nextest.tar.gz \
    && echo "$sum  /tmp/cargo-nextest.tar.gz" | sha256sum --check \
    && tar -xzf /tmp/cargo-nextest.tar.gz -C /usr/local/cargo/bin cargo-nextest \
    && rm /tmp/cargo-nextest.tar.gz
RUN case "$(uname -m)" in \
      x86_64) sum=aa73c7270c997894038c0d8f0d297865f57569e1687883322bc431713d4bb9dd ;; \
      aarch64) sum=3e47fc4a71b1ddd1b4294a0de9d34498922fdf41e3cd0bda5df3954e3d6deb9d ;; \
      *) echo "sccache: no pinned build for $(uname -m)" >&2; exit 1 ;; \
    esac \
    && curl --fail --location --silent --show-error "https://github.com/mozilla/sccache/releases/download/v0.19.0/sccache-v0.19.0-$(uname -m)-unknown-linux-musl.tar.gz" --output /tmp/sccache.tar.gz \
    && echo "$sum  /tmp/sccache.tar.gz" | sha256sum --check \
    && tar -xzf /tmp/sccache.tar.gz -C /tmp --strip-components=1 \
    && install /tmp/sccache /usr/local/bin/sccache \
    && rm /tmp/sccache /tmp/sccache.tar.gz
COPY crates/harness/src/compile_cache/mod.rs /tmp/aoe-compile-cache.rs
RUN rustc --edition=2024 -O /tmp/aoe-compile-cache.rs -o /usr/local/bin/aoe-compile-cache \
    && rm /tmp/aoe-compile-cache.rs
WORKDIR /workspace
