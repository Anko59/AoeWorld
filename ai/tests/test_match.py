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


def test_host_mode_plays_given_tokens_and_writes_a_report(server, secret, tmp_path):
    """`python -m aoe_ai host`, the entry the scale harness uses."""
    import json

    from aoe_ai.__main__ import main

    info = aoeworld.start_match(server, secret, ["ai", "ai"], tiles_per_side=500, seed=4)
    tokens_file = tmp_path / "tokens.json"
    tokens_file.write_text(json.dumps([slot.token for slot in info.slots]))
    report = tmp_path / "report.json"
    code = main(
        [
            "host",
            "--url", server,
            "--tokens-file", str(tokens_file),
            "--max-seconds", "4",
            "--report", str(report),
        ]
    )
    assert code == 0
    document = json.loads(report.read_text())
    assert [bot["slot"] for bot in document["bots"]] == [0, 1]
    assert all(bot["decisions"] > 0 for bot in document["bots"])


def test_scripted_raid_draws_the_field_army_home():
    """The reserve answers a raid while the siege force keeps its targets."""
    import http.client
    import json
    import urllib.parse
    from pathlib import Path
    from types import SimpleNamespace

    from aoe_ai import Brain
    from aoe_ai.arena import ArenaServer, SECRET, play
    from aoe_ai.context import Context
    from aoe_ai.geometry import dist

    root = Path(__file__).resolve().parents[2]
    binary = Path(os.environ.get("AOE_SERVER_BIN", root / "target" / "debug" / "aoe-server"))
    cfg = Config(attack_army_size=30)
    report = {}

    with ArenaServer(binary, root) as arena:
        address = urllib.parse.urlsplit(arena.url)
        connection = http.client.HTTPConnection(address.hostname, address.port, timeout=30)

        def request(method, path, body=None):
            connection.request(method, path, None if body is None else json.dumps(body),
                               {"x-aoeworld-admin-token": SECRET, "content-type": "application/json"})
            response = connection.getresponse()
            data = response.read()
            assert response.status == 200, data
            return json.loads(data)

        setup = request("POST", "/match", {
            "slots": ["human", "ai", "ai"], "flat": {"tiles_per_side": 350, "seed": 7},
            "seed": 7, "start_tiles": [[95, 175], [190, 175], [190, 85]],
            "start_units": {"militia": 24},
        })
        info = SimpleNamespace(slots=[SimpleNamespace(**slot) for slot in setup["slots"]])
        players = request("GET", "/admin/match")["players"]
        centers = [p["town_centers"][0] for p in players]
        home = round(centers[1]["x"]), round(centers[1]["y"])
        away = round(centers[2]["x"]), round(centers[2]["y"]) + 5
        human_start = info.slots[0].start_tile
        # Use opposite sides of the TC for staging and the scripted raid.
        edge = 1 if human_start[0] > home[0] else -1
        staging = home[0] + edge * 34, home[1] + 10
        approaches = [(home[0] + edge * dx, home[1] + dy)
                      for dx, dy in ((5, 0), (7, 2), (7, -2), (12, -7), (12, 7))]

        class Defender(Brain):
            siege_id = None
            reserve = frozenset()
            engaged_at_hit = frozenset()
            reserve_fought = None

            def decide(self, client, turn):
                world = turn.world
                soldiers = [u for u in world.my_units(aoeworld.UnitKind.MILITIA) if u.alive]
                if "staged" not in report and soldiers:
                    client.formation_move([u.id for u in soldiers], *away)
                    report["staged"] = turn.tick
                if "hit" not in report:
                    outpost = next((b for b in world.known_enemy_buildings()
                                    if b.owner == 2 and b.visible and b.kind == aoeworld.BuildingKind.TOWN_CENTER), None)
                    if outpost and self.siege_id is None:
                        self.siege_id = outpost.id
                        force = sorted(soldiers, key=lambda u: u.id)
                        self.reserve = {u.id for u in force[12:]}
                        for soldier in force[:12]:
                            client.attack(soldier.id, outpost.id)
                        report["besieged"] = turn.tick
                    if self.siege_id is not None:
                        deployed = {u.id for u in soldiers if dist(u.tile, home) > 50
                                    and u.attack_target == self.siege_id}
                        report["occupied"] = len(deployed)
                        if any(e.kind == "under_attack" and e.is_building for e in turn.events):
                            self.engaged_at_hit = deployed
                            report["hit"] = turn.tick
                            report["occupied_at_hit"] = len(deployed)
                if "raided" not in report:
                    return
                ctx = Context(client, turn, self.board, self.config)
                self.absorb(ctx)
                self.combat.step(ctx)
                if "hit" in report and turn.tick <= report["hit"] + 140 * turn.world.tick_hz:
                    returned = [u for u in soldiers if u.id in self.reserve
                                and (dist(u.tile, home) < 40 or u.attack_target is not None)]
                    report["returned"] = max(report.get("returned", 0), len(returned))
                    if self.reserve_fought is None:
                        self.reserve_fought = set()
                    fighting = {u.id for u in returned if u.attack_target is not None}
                    self.reserve_fought.update(fighting)
                    report["reserve_fighting"] = len(self.reserve_fought)
                    report["reserve_size"] = len(self.reserve)
                    if turn.tick == report["hit"]:
                        report["siege_reordered_at_hit"] = len(self.engaged_at_hit & ctx.busy)

        class Raider(Brain):
            march_sequence = None
            attempts = 0

            def decide(self, client, turn):
                soldiers = {u.id for u in turn.world.my_units(aoeworld.UnitKind.MILITIA) if u.alive}
                if "raider_staged" not in report and len(soldiers) >= 10:
                    self.raiders = sorted(soldiers)[:10]
                    client.formation_move(self.raiders, *staging)
                    report["raider_staged"] = turn.tick
                if "raided" not in report:
                    if report.get("occupied", 0) < 10 or "raider_staged" not in report:
                        return
                    report["raided"] = turn.tick
                if "targeted" in report:
                    return
                target = next((b for b in turn.world.known_enemy_buildings()
                               if b.owner == 1 and b.visible and b.kind == aoeworld.BuildingKind.TOWN_CENTER), None)
                if target:
                    for soldier in self.raiders:
                        if soldier in soldiers:
                            client.attack(soldier, target.id)
                    report["targeted"] = turn.tick
                    return
                for event in turn.events:
                    if event.kind == "order_result" and event.sequence == self.march_sequence:
                        report.setdefault("raid_verdicts", []).append(str(event.result))
                        if event.result != aoeworld.CommandResult.ACCEPTED:
                            self.march_sequence = None
                if self.march_sequence is None and self.attempts < len(approaches):
                    at = approaches[self.attempts]
                    self.attempts += 1
                    self.march_sequence = client.formation_move(self.raiders, *at, attack_move=True)

        class Outpost(Brain):
            sequence = None
            attempts = 0

            def decide(self, client, turn):
                if "outpost_staged" in report:
                    return
                for event in turn.events:
                    if event.kind == "order_result" and event.sequence == self.sequence:
                        report.setdefault("outpost_verdicts", []).append(str(event.result))
                        if event.result == aoeworld.CommandResult.ACCEPTED:
                            report["outpost_staged"] = turn.tick
                            return
                        self.sequence = None
                targets = [(away[0] + dx, away[1] + dy)
                           for dx, dy in ((45, 0), (0, -45), (40, 18), (-45, 0), (55, -38))]
                if self.sequence is not None or self.attempts >= len(targets):
                    return
                soldiers = turn.world.my_units(aoeworld.UnitKind.MILITIA)
                if soldiers:
                    self.sequence = client.formation_move([u.id for u in soldiers], *targets[self.attempts])
                    self.attempts += 1

        brains = [Raider(cfg), Defender(cfg), Outpost(cfg)]

        def observe(tick, _host, _match):
            if tick >= 6 * 60 * 20:
                state = request("GET", "/admin/match")
                report["combat"] = [(p["kills"], p["losses"]) for p in state["players"]]

        result = play(arena.url, 3, 6.1, 350, 7, config=cfg, match_info=info,
                      brain_factory=brains.__getitem__, observe=observe)
        connection.close()

    print(report, flush=True)
    assert all(bot["errors"] == 0 for bot in result["bots"]), result
    assert "hit" in report and report["occupied_at_hit"] >= 10, report
    assert report["siege_reordered_at_hit"] == 0, report
    assert report.get("returned", 0) >= 8, report
    assert report["reserve_fighting"] > 0, report
    kills, losses = report["combat"][1]
    assert kills > losses, report


def test_scripted_ambush_gets_an_answer_from_the_marching_army():
    """Exercise the bot, SDK, and actual orders on generated terrain."""
    from aoe_ai import Brain
    from aoe_ai.arena import ArenaServer, play
    from aoe_ai.context import Context
    from aoe_ai.geometry import centroid, dist
    from aoe_ai.play.combat.march import march
    from pathlib import Path

    root = Path(__file__).resolve().parents[2]
    binary = Path(os.environ.get("AOE_SERVER_BIN", root / "target" / "debug" / "aoe-server"))
    cfg = Config(attack_army_size=24)
    seen = {"marched": False, "attackers_ordered": False, "fighting": set(), "hits": 0}
    initial = 18

    with ArenaServer(binary, root) as arena:
        import json
        import urllib.request
        from types import SimpleNamespace

        from aoe_ai.arena import SECRET

        def admin(path, payload=None):
            req = urllib.request.Request(
                arena.url + path, None if payload is None else json.dumps(payload).encode(),
                headers={"x-aoeworld-admin-token": SECRET, "content-type": "application/json"},
            )
            with urllib.request.urlopen(req, timeout=30) as reply:
                return json.load(reply)

        setup = admin("/match", {
            "slots": ["human", "ai"], "flat": {"tiles_per_side": 350, "seed": 7},
            "seed": 7, "start_tiles": [[165, 175], [245, 175]],
            "start_units": {"militia": initial},
        })
        info = SimpleNamespace(slots=[SimpleNamespace(**slot) for slot in setup["slots"]])
        x, y = info.slots[0].start_tile
        ambush = (x + 25, y + 12)

        class Ambushers(Brain):
            def decide(self, client, turn):
                if seen["attackers_ordered"]:
                    return
                soldiers = turn.world.my_units(aoeworld.UnitKind.MILITIA)
                if soldiers:
                    client.formation_move([u.id for u in soldiers], *ambush, attack_move=True)
                    seen["attackers_ordered"] = True

        class Marchers(Brain):
            def decide(self, client, turn):
                ctx = Context(client, turn, self.board, self.config)
                soldiers = [u for u in ctx.army if u.kind == aoeworld.UnitKind.MILITIA and u.alive]
                if not soldiers:
                    return
                self.absorb(ctx)
                seen["fighting"].update(u.id for u in soldiers if u.attack_target is not None)
                ids = {u.id for u in soldiers}
                seen["hits"] += sum(e.kind == "under_attack" and e.id in ids for e in ctx.events)
                if not seen["marched"]:
                    middle = centroid([u.tile for u in soldiers])
                    goal = (ambush[0] - 14, ambush[1])
                    march(ctx, soldiers, middle, goal)
                    seen["marched"] = bool(ctx.orders)
                    self.board.attacking = True
                    self.board.attack_launch_size = initial
                elif min(dist(u.tile, ambush) for u in soldiers) < 30:
                    self.combat.step(ctx)
                else:
                    march(ctx, soldiers, soldiers[0].tile, ambush)

        brains = [Ambushers(cfg), Marchers(cfg)]
        result = play(arena.url, 2, 2.5, 350, 7, config=cfg, match_info=info,
                      brain_factory=brains.__getitem__)
        data = admin("/admin/match")["players"]
        combat = [(p["kills"], p["losses"]) for p in data]

    print({"ambush_combat": combat, "marchers_fighting": len(seen["fighting"]),
           "marchers_initial": initial, "hits": seen["hits"]}, flush=True)
    assert all(bot["errors"] == 0 for bot in result["bots"]), result
    assert seen["marched"] and seen["hits"] > 0, seen
    assert len(seen["fighting"]) >= initial // 2, seen
    assert combat[1][0] > 0, combat
