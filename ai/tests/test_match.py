"""Integration: two complete players on a small map for several game minutes."""

import os

import aoeworld

from aoe_ai import Config
from aoe_ai.host import Host

GAME_MINUTES = float(os.environ.get("AOE_AI_MINUTES", "4"))


def test_two_bots_play_a_small_map_without_crashing_and_both_grow(server, secret):
    info = aoeworld.start_match(server, secret, ["ai", "ai"], tiles_per_side=500, seed=3)
    tokens = [slot.token for slot in info.slots]
    host = Host(server, tokens, Config(decision_hz=2.0))
    ticks = int(GAME_MINUTES * 60 * 20)
    reports = host.run(until_tick=ticks, max_seconds=600)

    assert len(reports) == 2
    for report in reports:
        assert report.errors == 0, f"slot {report.slot} raised {report.errors} times"
        assert not report.closed
        assert report.last_tick >= ticks
        # Growth: more than the starting three villagers and more than a Town Center.
        assert report.villagers > 6, report
        assert report.buildings > 2, report
        assert report.population > 8, report
        # Orders are accepted: the server's verdicts are overwhelmingly yes.
        assert report.accepted > 20, report
        assert report.accepted > 2 * report.rejected, report
    # The two players are different seats of one match.
    assert {r.player for r in reports} == {0, 1}


def test_a_host_survives_a_bot_that_raises(server, secret):
    info = aoeworld.start_match(server, secret, ["ai", "ai"], tiles_per_side=500, seed=5)

    class Broken:
        config = Config()
        board = type("M", (), {"stats": {}})()

        def decide(self, client, decision):
            raise RuntimeError("boom")

    from aoe_ai import Brain

    host = Host(
        server,
        [slot.token for slot in info.slots],
        brain_factory=lambda slot: Broken() if slot == 0 else Brain(),
    )
    reports = host.run(until_tick=200, max_seconds=60)
    assert reports[0].errors > 0 and reports[1].errors == 0
    assert reports[1].decisions > 0
