ARG BUILD_IMAGE
FROM ${BUILD_IMAGE} AS artifacts
FROM debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251
COPY --from=artifacts --chown=65532:65532 /source/target/release/aoe-server /app/aoe-server
COPY --from=artifacts --chown=65532:65532 /source/web /app/web
WORKDIR /app
USER 65532:65532
ENV AOE_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/app/aoe-server"]
