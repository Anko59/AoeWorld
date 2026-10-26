"""The full Dark-to-Feudal game on the AI arena: fast, seeded and repeatable.

Two ``aoe_ai`` players share a small flat map on a server in arena mode (the
simulation runs in lockstep with the bots, not on a clock, see
``aoe_ai/arena.py``). Within the game-time budget every bot must reach Feudal Age
and field at least ``SOLDIERS`` soldiers at once, and a blow of our own army
must land: an enemy unit or building whose health fell while our soldiers were
in range. All of it is read from the SDK's world view, never from the server.
"""

import os
from pathlib import Path

from aoe_ai.arena import run_seeds

ROOT = Path(__file__).resolve().parents[2]
SEEDS = [int(seed) for seed in os.environ.get("AOE_AI_SEEDS", "1,2,3,4,5").split(",")]
GAME_MINUTES = float(os.environ.get("AOE_AI_FULL_MINUTES", "30"))
#: Soldiers every bot must field at once (docs/ai-sdk.md "Measured play").
SOLDIERS = 10


def test_bots_reach_feudal_build_an_army_and_land_an_attack_on_every_seed():
    binary = Path(os.environ.get("AOE_SERVER_BIN", ROOT / "target" / "debug" / "aoe-server"))
    assert binary.exists(), f"build the server first: {binary}"
    results = run_seeds(binary, ROOT, SEEDS, bots=2, minutes=GAME_MINUTES, size=300, soldiers=SOLDIERS)
    for result in results:
        print(result, flush=True)
        for bot in result["bots"]:
            assert bot["errors"] == 0, result
            assert bot["feudal_minute"] is not None, result
            assert bot["army_peak"] >= SOLDIERS, result
        assert any(bot["attacks_landed"] > 0 for bot in result["bots"]), f"no attack landed: {result}"


def test_one_bot_reaches_castle_and_fields_castle_units_through_the_public_sdk():
    binary = Path(os.environ.get("AOE_SERVER_BIN", ROOT / "target" / "debug" / "aoe-server"))
    result = run_seeds(binary, ROOT, [1], bots=1, minutes=30, size=300)[0]
    print(result, flush=True)
    bot = result["bots"][0]
    assert bot["errors"] == 0, result
    assert bot["castle_minute"] is not None, result
    assert bot["castle_units_peak"] > 0, result
