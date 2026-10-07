# Credentials

**Every credential this project uses is in the person's keyring** (the
desktop Secret Service, read with `secret-tool`). None is in the repository,
in `.env` files, or exported in the shell by default. A missing environment
variable does **not** mean you lack access: look it up in the keyring first,
and never report "no credentials" before you have.

```sh
# Run the one command that needs it with the variable set (never echo it):
OPENROUTER_API_KEY=$(secret-tool lookup service codex-api name OPENROUTER_API_KEY) make <target> …

# What is available (names only; the grep drops the secret values):
secret-tool search --all service codex-api 2>/dev/null | grep '^attribute.name'
```

| Variable | Keyring lookup | Used for |
|---|---|---|
| `OPENROUTER_API_KEY` | `service codex-api name OPENROUTER_API_KEY` | Showcase narration (Gemini TTS on OpenRouter) |
| `LITELLM_MASTER_KEY` | `service codex-api name LITELLM_MASTER_KEY` | The local LiteLLM gateway (DeepSeek Harness, GLM reviewers) |
| `DEEPSEEK_API_KEY` | `service codex-api name DEEPSEEK_API_KEY` | DeepSeek models |
| `ZAI_API_KEY` | `service codex-api name ZAI_API_KEY` | GLM models (Z.ai) |
| `MISTRAL_API_KEY` | `service codex-api name MISTRAL_API_KEY` | Mistral models |
| GitHub | `gh` reads its own token from the keyring | `make ship`, issues; never needs a variable |

Rules:

- Set a secret only in the environment of the command that needs it. Never
  print it, write it to a file, commit it, put it in a URL or paste it into a
  chat or pull request.
- A secret that is not in the keyring is a question for the person; name the
  variable and the lookup you tried.
