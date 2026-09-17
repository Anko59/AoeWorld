FROM aoeworld/rust-tools:1.93.1
RUN apt-get update && apt-get install -y --no-install-recommends valgrind binaryen && rm -rf /var/lib/apt/lists/*
RUN cargo install gungraun-runner --version 0.19.4 --locked
WORKDIR /workspace
