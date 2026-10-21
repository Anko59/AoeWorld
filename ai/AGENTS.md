# Example AI changes

`aoe_ai` plays through the public `aoeworld` API only: no server internals, no
information the slot's fog of war would not give a human. Keep managers small
and single-purpose, put every tunable in `config.py`, and cover decision logic
with fake-world tests (`tests/test_logic.py`) plus the integration test in
`tests/test_match.py`. Run `make sdk-python-test`. Read
[the AI SDK guide](../docs/ai-sdk.md).
