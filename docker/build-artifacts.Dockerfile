FROM aoeworld/rust-tools:1.99.1-sccache0.19.0-wasm0.2.128-p1
ARG SOURCE_SHA
WORKDIR /source
COPY . .
RUN test -n "$SOURCE_SHA" && AOE_BUILD_SHA="$SOURCE_SHA" CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 cargo build --locked --release -p aoe-server && cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client && wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
RUN cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client --features diagnostics && wasm-bindgen --target web --out-dir web/diagnostics-pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
RUN cargo run --locked -p aoe-harness -- assets browser-shaders
