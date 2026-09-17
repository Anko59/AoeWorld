FROM aoeworld/rust-tools:1.93.1
RUN cargo install cargo-deny --version 0.20.2 --locked
WORKDIR /workspace
