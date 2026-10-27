# Model ledger and usage

Two commands answer, without asking an agent, which model to route work to.
Both run on the host in the main session only (they use `gh` and, for
`usage`, the keyring) and are implemented in `crates/harness/src/ledger/`.

```sh
make ledger   # which model did what, with measured results
make usage    # what is left of each provider, then the last 7 days of the ledger
```

## `make ledger`

For each pull request merged into `dev` the ledger records:

| Field | Source |
|---|---|
| Implementer models | The description's `Written by …` line, then the `Co-Authored-By` trailers of the pull request's commits. Bots are not models. |
| Review tier, reviewers, reviewer model, grade, confirmed findings, rounds | The review badge `make ship` renders (`**Review: 8/10** (high tier · 3 reviewers · 2 rounds · claude-sonnet-5-5 high): 4 confirmed …`). |
| First-review grade | The badge when the review took one round; for a closing review, its `approved N/10`. After several rounds the badge keeps only the last grade, so the first one is `unknown`. |
| Closing review | The badge reads `Closing review`. |
| Split | The title starts with `Split`, or the description says `split from #N` (a heuristic). |
| Reverted | A later commit on dev with the subject `Revert "… (#N)"`. |

The description is the source because it exists for every pull request and
every machine. The evidence files under `<git common dir>/aoe-ship/` exist only
on the machine that shipped, are keyed by branch commits that a merge does
not keep, and hold one file per round, so they cannot give a complete or
reproducible record. A pull request merged without a review badge has no
review fields and counts as unreviewed.

The cache is `.cache/ledger/ledger.json` (not committed, one current format;
a file of another format is dropped and rebuilt). A run lists the newest
`LEDGER_WINDOW` (default 200) merged pull requests with one API call and fetches
the ones the cache lacks, newest first, in at most `LEDGER_MAX_CALLS` (default
50) API calls in total. Older ones wait for the next run. A failed call stops
the run, keeps what it fetched and says so.

The table has one row per model: pull requests, reviewed, mean first-review
grade, share that passed the first review (a closing review follows an
approval, so it counts as passed; a full review passed if it took one round at
8/10 or more), mean confirmed findings, closing reviews, splits and reverts. A
second table groups by reviewer model. A pull request counts for every model on
it, so rows add up to more than the number of pull requests. A mean with no
data prints `unknown`.

## `make usage`

Reads what can be read without a new credential. Anything else prints
`unknown: <reason>`; nothing is guessed.

| Provider | Read from | Remaining |
|---|---|---|
| Claude Code | Token counts per model in the last 7 days from `~/.claude/projects/**/*.jsonl` | `unknown`: the plan quota is held by Anthropic and shown only by `/usage` in the interactive client |
| Codex | The newest rate-limit snapshot in `~/.codex/sessions` (`token_count` events): percent used and reset time per window | Percent left; `unknown` when the window has already reset or no snapshot exists |
| OpenRouter | `GET /api/v1/credits` with `OPENROUTER_API_KEY` from the environment or the keyring ([credentials](credentials.md)) | Credits bought minus used; `unknown` when the key is missing or refused |

The key goes to `curl` on standard input, never into a command line, a file or
the output. Data is sent only to GitHub and OpenRouter.

GLM through pi and LiteLLM has no usage endpoint the harness can read without
a new credential, so it has no section.

The Codex reading is the service's last report to a session, not a fresh
query; its timestamp is printed beside it.
