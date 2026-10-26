# SDK changes

The SDK is a client of the same wire protocol as the browser: it may know only
what fog of war lets its slot see, and it sends ordinary orders. Never add a
back door that reads simulation state. Keep it async-runtime agnostic apart
from tokio tasks, keep per-bot work proportional to what that bot sees, and
cover changes with tests against a local server. Read
[the AI SDK guide](../../docs/ai-sdk.md) and run `make test-unit`.
