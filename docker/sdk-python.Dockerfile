FROM aoeworld/rust-tools:1.99.1
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-venv=3.11.2-1+b1 python3-pip=23.0.1+dfsg-1 \
    && rm -rf /var/lib/apt/lists/*
RUN python3 -m venv /opt/sdk-venv \
    && /opt/sdk-venv/bin/pip install --no-cache-dir \
    maturin==1.15.0 pytest==9.1.1 pytest-asyncio==1.4.0
ENV PATH=/opt/sdk-venv/bin:$PATH
WORKDIR /workspace
