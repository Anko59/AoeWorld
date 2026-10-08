# Media tools for `make showcase` (docs/showcase.md): ffmpeg and ffprobe to
# measure narration and mix it into showcase videos. Pinned base and package.
FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg=7:5.1.9-0+deb12u1 \
    && rm -rf /var/lib/apt/lists/*
