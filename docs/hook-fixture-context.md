# Hook test fixture context

The hook manager still rejects every inherited `GIT_CONFIG` / `GIT_CONFIG_*`
override, including global/no-system settings and fsmonitor injection. This
production refusal protects canonical hook paths; the scanner's fixed safe Git
settings are also unchanged. Do not remove either guard to make a baseline pass.

Hook unit fixtures need their own context rather than ambient scanner config.
When inherited Git variables exist, the unit entry invokes that **same exact test**
in the current compiled test executable through the bounded process runner, with
Git variables cleared on the child only. The child executes every original
assertion. The parent requires actual success, complete output and exactly one
passed/unignored test; zero selected tests is not success. No process-global env
mutation, marker bypass, ignore, weaker assertion or test-executable substitute.

A regression suite invokes all nine hook fixtures under fixed scanner-style config
and poisoned directory/index variables, verifying actual cleared-child execution.
CLI fixture constructors similarly clear ambient Git variables before deliberately
adding each test's own overrides. Explicit config injection after this constructor
must still make both installer and checker fail without changing owned sentinels.

Validate the whole locked harness package under the scanner’s unchanged Git
environment. Fixture isolation does not establish independent worker authority
or compiled cache provenance.
