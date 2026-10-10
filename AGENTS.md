# AoeWorld working agreement

Read [the engineering guide](docs/agent-engineering.md), then the scoped
`AGENTS.md` for any subsystem you change. Use Dockerized Make targets for
compilation and checks. Keep original game assets outside Git and images.
The provider-neutral task guides in `skills/` route focused work to the same
canonical documentation and commands. Claude Code, Codex, DeepSeek Harness and pi
are judged by the same hooks; see [agent runtimes](docs/agent-runtimes.md).

Before committing, run `make hooks-install`, `make hooks-check`, and the focused
gate plus `make preflight`. Publish only with `make ship` ([shipping](docs/shipping.md)). Never bypass hooks or reduce a gate/baseline to obtain
a pass. Human-authored text files must stay within 500 lines, and a directory
may directly contain at most 14 code/config files. Place substantive command
logic in Rust, not Make, shell, or workflow YAML.

No backward compatibility before v1.0 (the owner's rule). There are no
players and no data worth keeping: change formats, generators, schemas and
protocols in place, keep exactly one current version, and regenerate local
data instead of migrating it. Do not add legacy or "prior" versions, opt-in
"V2" profiles beside the old path, version-gated branches, or golden fixtures
that freeze old output; delete such code when you touch it. Old requirements
saying otherwise (for example in #200) are void.

Credentials (API keys, tokens) are in the keyring, never in the repository or
the default environment: look them up with `secret-tool` before concluding you
lack access ([credentials](docs/credentials.md)).

For work outside this PR's scope, notice it and don't fix it in this PR.
The main session, testers and implementers may file it with `make issue`; review
findings are filed later by `make ship` (#174). Use `make next` to select an
unblocked task. `make issue` takes one bounded document on standard input:
title, optional `labels: a, b` line, a blank line, then the body. For example:
`printf 'Follow-up title\n\nFollow-up details\n' | make issue`.

Report the exact revision, commands, results, and limits. Keep the working tree
clean and commit intended changes. Synthetic workloads do not demonstrate
finished RTS performance or dedicated hardware qualification.
