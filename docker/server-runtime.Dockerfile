ARG BUILD_IMAGE
FROM ${BUILD_IMAGE} AS artifacts
FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
COPY --from=artifacts --chown=65532:65532 /source/target/release/aoe-server /app/aoe-server
COPY --from=artifacts --chown=65532:65532 /source/web /app/web
WORKDIR /app
USER 65532:65532
ENV AOE_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/app/aoe-server"]
