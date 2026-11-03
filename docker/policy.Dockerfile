FROM aoeworld/rust-tools:1.99.1-sccache0.19.0
RUN cargo install cargo-deny --version 0.20.2 --locked
WORKDIR /workspace
