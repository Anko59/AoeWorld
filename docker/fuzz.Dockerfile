FROM aoeworld/rust-tools:1.99.1-sccache0.19.0-wasm0.2.128-p1
RUN rustup toolchain install nightly-2026-09-01 --profile minimal \
    && cargo install cargo-fuzz --version 0.13.2 --locked
WORKDIR /workspace
