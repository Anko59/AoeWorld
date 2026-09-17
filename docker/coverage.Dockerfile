FROM aoeworld/rust-tools:1.93.1
RUN rustup component add llvm-tools-preview && cargo install cargo-llvm-cov --version 0.9.1 --locked
WORKDIR /workspace
