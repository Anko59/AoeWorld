# AoeWorld working agreement

Read [the engineering guide](docs/agent-engineering.md), then the scoped
`AGENTS.md` for any subsystem you change. Use Dockerized Make targets for
compilation and checks. Keep original game assets outside Git and images.
The provider-neutral task guides in `skills/` route focused work to the same
canonical documentation and commands.

Before committing, run `make hooks-install`, `make hooks-check`, and the focused
gate plus `make preflight`. Never bypass hooks or reduce a gate/baseline to obtain
a pass. Human-authored text files must stay within 500 lines, and a directory
may directly contain at most 14 code/config files. Place substantive command
logic in Rust, not Make, shell, or workflow YAML.

Report the exact revision, commands, results, and limits. Keep the working tree
clean and commit intended changes. Synthetic workloads do not demonstrate
finished RTS performance or dedicated hardware qualification.
