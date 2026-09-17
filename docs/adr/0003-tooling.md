# 0003: Rust harness behind Make

Make is the Dockerized public interface. Typed Rust tooling owns policy,
process supervision, and reports. Hooks and workflows dispatch those commands.
