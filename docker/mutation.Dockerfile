FROM rust:1.93.1-trixie@sha256:ecbe59a8408895edd02d9ef422504b8501dd9fa1526de27a45b73406d734d659
RUN rustup target add wasm32-unknown-unknown && rustup component add clippy rustfmt
RUN curl --fail --location --silent --show-error \
      https://github.com/sourcefrog/cargo-mutants/releases/download/v27.1.0/cargo-mutants-x86_64-unknown-linux-gnu.tar.gz \
      --output /tmp/cargo-mutants.tar.gz \
    && echo 'dfe6dc37d0342c891d2829b5a695aa57c2d0edecef7e7d0399a30cc6e206411e  /tmp/cargo-mutants.tar.gz' | sha256sum --check \
    && tar -xzf /tmp/cargo-mutants.tar.gz -C /usr/local/cargo/bin cargo-mutants \
    && rm /tmp/cargo-mutants.tar.gz
WORKDIR /workspace
