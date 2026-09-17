FROM aoeworld/rust-tools:1.93.1
ARG SOURCE_SHA
WORKDIR /source
COPY . .
RUN test -n "$SOURCE_SHA" && AOE_BUILD_SHA="$SOURCE_SHA" cargo build --locked --release -p aoe-server && cargo build --locked --release --target wasm32-unknown-unknown -p aoe-client && wasm-bindgen --target web --out-dir web/pkg --out-name aoe_client target/wasm32-unknown-unknown/release/aoe_client.wasm
