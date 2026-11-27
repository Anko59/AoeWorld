FROM aoeworld/rust-tools:1.99.1-sccache0.19.0-wasm0.2.128-p1
RUN cargo install cargo-deny --version 0.20.2 --locked
WORKDIR /workspace
