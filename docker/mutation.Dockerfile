FROM rust:1.98.1-trixie@sha256:a8a5f0a1e5fe7dfe1d352591e4a1c7dd2c08fd70475cae872cf3458ba0df0546
RUN rustup target add wasm32-unknown-unknown && rustup component add clippy rustfmt
RUN curl --fail --location --silent --show-error \
      https://github.com/sourcefrog/cargo-mutants/releases/download/v27.1.0/cargo-mutants-x86_64-unknown-linux-gnu.tar.gz \
      --output /tmp/cargo-mutants.tar.gz \
    && echo 'dfe6dc37d0342c891d2829b5a695aa57c2d0edecef7e7d0399a30cc6e206411e  /tmp/cargo-mutants.tar.gz' | sha256sum --check \
    && tar -xzf /tmp/cargo-mutants.tar.gz -C /usr/local/cargo/bin cargo-mutants \
    && rm /tmp/cargo-mutants.tar.gz
WORKDIR /workspace
