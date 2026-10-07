# Credentials

**Every credential this project uses is in the person's keyring** (the
desktop Secret Service, read with `secret-tool`). None is in the repository,
in `.env` files, or exported in the shell by default. A missing environment
variable does **not** mean you lack access: look it up in the keyring first,
and never report "no credentials" before you have.

```sh
# Run the one command that needs it with the variable set (never echo it):
OPENROUTER_API_KEY=$(secret-tool lookup service codex-api name OPENROUTER_API_KEY) <command>

# What is available (names only: secret-tool prints attributes on stderr, and the grep drops the secret values):
secret-tool search --all --unlock service codex-api 2>&1 | grep '^attribute.name' | sort -u
```

| Variable | Keyring lookup | Used for |
|---|---|---|
| `OPENROUTER_API_KEY` | `service codex-api name OPENROUTER_API_KEY` | Narrated showcase videos (Gemini TTS on OpenRouter) |
| `LITELLM_MASTER_KEY` | `service codex-api name LITELLM_MASTER_KEY` | The person's local LiteLLM gateway, which DeepSeek Harness and the GLM models go through |
| `DEEPSEEK_API_KEY`, `ZAI_API_KEY`, `MISTRAL_API_KEY` | `service codex-api name <VAR>` | Model providers behind that gateway; configured outside this repository |
| `GITHUB_TOKEN` | `service gh:github.com username <GitHub login>` (gh's own keyring entry; `gh auth token` is refused by the hooks) | Commands that call the GitHub API themselves, such as `make repo-policy-check`; `gh` itself (`make ship`, issues) needs nothing |

The variables above are read on the host. A Make target that runs its
command in Docker only sees the variables its recipe forwards (`-e …`), so a
secret exported for `make` does not reach it unless that target forwards it.

Rules:

- Tester, implementer and reviewer subagents never handle credentials:
  `secret-tool` is not on their command allow-list, so the hooks refuse it. A
  step that needs a secret is run by the main session.
- Set a secret only in the environment of the command that needs it. Never
  print it, write it to a file, commit it, put it in a URL or paste it into a
  chat or pull request.
- A secret that is not in the keyring blocks only the step that needs it: open
  an issue naming the variable and the lookup you tried, and carry on with other
  work.
