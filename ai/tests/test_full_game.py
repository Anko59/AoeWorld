"""Live proof: bots play a full Dark-to-Feudal game and fight each other.

Two to four ``aoe_ai`` players share a small flat map on the real server
(3x speed: the fixture runs it at 60 ticks a second). Within the game-time
budget every bot must reach Feudal Age and field an army of at least ten
at once, and a blow of our own army must land: an enemy unit or building seen below
full health while an attack is under way. All of it
is read from the SDK's world view and events, never from the server.
"""

import os

import aoeworld

from aoe_ai import Config
from aoe_ai.host import Host

BOTS = int(os.environ.get("AOE_AI_BOTS", "2"))
GAME_MINUTES = float(os.environ.get("AOE_AI_FULL_MINUTES", "30"))
TICKS_PER_MINUTE = 20 * 60


def done(reports):
    return (
        all(r.age == "FEUDAL" and r.stats.get("army_peak", 0) >= 10 for r in reports)
        and any(r.stats.get("attack_hits", 0) > 0 for r in reports)
    )


def test_bots_reach_feudal_build_an_army_and_land_an_attack(server, secret):
    info = aoeworld.start_match(server, secret, ["ai"] * BOTS, tiles_per_side=300, seed=3)
    host = Host(server, [slot.token for slot in info.slots], Config(decision_hz=2.0))
    reports = host.run(
        until_tick=int(GAME_MINUTES * TICKS_PER_MINUTE), max_seconds=1500, stop=done,
        progress=lambda rs: print([(r.last_tick // 1200, r.age, r.villagers, r.army, dict(r.stats)) for r in rs], flush=True),
        progress_every=60,
    )
    for report in reports:
        feudal = report.stats.get("feudal_tick")
        print(
            f"bot {report.slot}: Feudal at {feudal / TICKS_PER_MINUTE if feudal else None} game minutes, "
            f"army peak {report.stats.get('army_peak')}, enemy units/buildings seen hurt during our attacks "
            f"{report.stats.get('enemies_wounded', 0)}, villagers {report.villagers}"
        )
        assert report.errors == 0 and not report.closed, report
        assert report.age == "FEUDAL", report
        assert report.stats.get("army_peak", 0) >= 10, report
    assert any(r.stats.get("attack_hits", 0) > 0 for r in reports), "no attack landed"
