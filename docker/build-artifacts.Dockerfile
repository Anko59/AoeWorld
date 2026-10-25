FROM aoeworld/rust-tools:1.99.1
ARG SOURCE_SHA
WORKDIR /source
COPY . .
RUN test -n "$SOURCE_SHA" && AOE_BUILD_SHA="$SOURCE_SHA" cargo build --locked --release -p aoe-server && cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client && wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
RUN cargo build --locked --profile wasm-release --target wasm32-unknown-unknown -p aoe-client --features diagnostics && wasm-bindgen --target web --out-dir web/diagnostics-pkg --out-name aoe_client target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm
RUN cargo run --locked -p aoe-harness -- assets browser-shaders
