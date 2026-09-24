FROM docker:29.8.1-cli@sha256:018edbc908e08fcc9dbf029c812c34251e9b4719e6f71ca0e5eae2a987d014ca AS dockercli
FROM aoeworld/rust-tools:1.93.1
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
WORKDIR /workspace
