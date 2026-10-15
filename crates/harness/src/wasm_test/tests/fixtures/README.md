# Runner diagnostics provenance

Downloaded `browser-diagnostics` from runs 38560009588 and 38603174196 into
the ignored worktree `.cache/webgl-fix/`. The pixel run's latest artifact is
from its successful rerun; the failing first-attempt artifact is ID 11915152835.

`dev-38560009588.txt` is verbatim cargo-7-1791979560168823727.log, with only
passing test lines and passing `Invoking test` entries removed. The source
starts with `[output capture truncated]`, mid-stack, and omits the first of
seven failure headers. Its Actions log has only a failure excerpt. It must
remain a no-retry regression; missing bytes cannot be invented.

`pixel-38603174196.txt` is verbatim cargo-7-1791997940798119040.log from the
first-attempt artifact, with only passing test lines and passing `Invoking
test` entries removed. All failure details, JS exceptions, duplicated console
panic output, summary, and stderr are retained.

`context-section.txt` is a complete surviving failure section from the dev
log, verbatim. Positive tests put it in an explicitly synthetic envelope,
sometimes with the real pixel section. This tests real failure details and
the revised mixed-failure rule, but does not establish classification of a
complete dev-run transcript: none was retained in the downloaded artifact.
