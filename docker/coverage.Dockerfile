FROM aoeworld/rust-tools:1.99.1-sccache0.19.0-wasm0.2.128-p1
RUN rustup component add llvm-tools-preview && cargo install cargo-llvm-cov --version 0.9.1 --locked
WORKDIR /workspace
