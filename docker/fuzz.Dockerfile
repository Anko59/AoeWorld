FROM aoeworld/rust-tools:1.93.1
RUN rustup toolchain install nightly-2026-09-01 --profile minimal \
    && cargo install cargo-fuzz --version 0.13.2 --locked
WORKDIR /workspace
