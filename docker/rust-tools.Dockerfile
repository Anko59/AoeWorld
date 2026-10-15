FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6
RUN apt-get update && apt-get install -y --no-install-recommends \
    gdal-bin=3.6.2+dfsg-1+b2 libgdal-dev=3.6.2+dfsg-1+b2 \
    libclang-dev=1:14.0-55.7~deb12u1 \
    libproj-dev=9.1.1-1+b1 proj-bin=9.1.1-1+b1 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown && rustup component add clippy rustfmt
RUN cargo install wasm-bindgen-cli --version 0.2.128 --locked
RUN curl --fail --location --silent --show-error https://get.nexte.st/0.9.144/linux --output /tmp/cargo-nextest.tar.gz \
    && echo '8a4f726272b0a1c499bd87ca3978bfbb1a8c20bb08ccf075b9996e2081bd1e1e  /tmp/cargo-nextest.tar.gz' | sha256sum --check \
    && tar -xzf /tmp/cargo-nextest.tar.gz -C /usr/local/cargo/bin cargo-nextest \
    && rm /tmp/cargo-nextest.tar.gz
WORKDIR /workspace
