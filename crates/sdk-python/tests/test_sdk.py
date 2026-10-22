import math
import threading
import time

import pytest

import aoeworld
from aoeworld import BuildingKind, ResourceKind, UnitKind


def new_match(server, secret, slots, seed=3):
    return aoeworld.start_match(server, secret, slots, tiles_per_side=500, seed=seed)


def test_start_match_assigns_tokens_and_spread_start_positions(server, secret):
    info = new_match(server, secret, ["human", "ai", "ai"])
    assert [slot.kind for slot in info.slots] == ["human", "ai", "ai"]
    assert [slot.player for slot in info.slots] == [0, 1, 2]
    assert len({slot.token for slot in info.slots}) == 3
    assert all(len(slot.token) == 48 for slot in info.slots)
    assert info.tiles_per_side == 500
    starts = [slot.start_tile for slot in info.slots]
    for i, a in enumerate(starts):
        for b in starts[i + 1 :]:
            assert max(abs(a[0] - b[0]), abs(a[1] - b[1])) > 60, starts


def test_match_requires_the_secret_and_valid_slots(server, secret):
    with pytest.raises(aoeworld.AoeError):
        aoeworld.start_match(server, "wrong-secret-wrong-secret", ["ai"])
    with pytest.raises(ValueError):
        aoeworld.start_match(server, secret, ["robot"])


def test_a_wrong_token_is_refused(server, secret):
    info = new_match(server, secret, ["ai"])
    bad = ("00" if info.slots[0].token[:2] != "00" else "01") + info.slots[0].token[2:]
    with pytest.raises(aoeworld.Refused):
        aoeworld.connect(server, bad)
    with pytest.raises(aoeworld.Refused):
        aoeworld.connect(server, "not-a-token")


def test_first_decision_exposes_a_typed_world(server, secret):
    info = new_match(server, secret, ["ai"])
    with aoeworld.connect(server, info.slots[0].token) as client:
        decision = client.wait_decision(timeout=10)
        assert decision is not None
        world = decision.world
        assert world.me == 0
        assert world.map_size == (500, 500)
        assert world.tick_hz == 20
        villagers = world.my_units(UnitKind.VILLAGER)
        assert len(villagers) == 3
        assert all(unit.mine and unit.kind == UnitKind.VILLAGER for unit in villagers)
        assert len(world.my_units(UnitKind.SCOUT)) == 1
        center = world.town_center()
        assert center is not None and center.kind == BuildingKind.TOWN_CENTER and center.complete
        economy = world.economy
        assert (economy.food, economy.wood, economy.gold, economy.stone) == (200, 200, 100, 200)
        assert economy.free_population > 0
        assert economy.can_afford(food=50) and not economy.can_afford(gold=1000)
        assert world.visible_enemies() == []
        assert world.known_enemy_buildings() == []
        assert isinstance(world.visibility(*center.center), aoeworld.Visibility)
        assert repr(villagers[0]).startswith("Unit(")


def test_orders_get_verdicts_as_events(server, secret):
    info = new_match(server, secret, ["ai"])
    with aoeworld.connect(server, info.slots[0].token, decision_hz=5) as client:
        first = client.wait_decision(timeout=10)
        center = first.world.town_center()
        villager = first.world.my_units(UnitKind.VILLAGER)[0]
        accepted = client.train(center.id, UnitKind.VILLAGER)
        rejected = client.attack(villager.id, 999_999)
        client.stop(villager.id)
        verdicts = {}
        deadline = time.time() + 15
        while time.time() < deadline and len(verdicts) < 3:
            decision = client.wait_decision(timeout=2)
            for event in decision.events if decision else []:
                if event.kind == "order_result":
                    verdicts[event.sequence] = event.result
        assert verdicts[accepted] == aoeworld.CommandResult.ACCEPTED
        assert verdicts[rejected] != aoeworld.CommandResult.ACCEPTED
        assert len(verdicts) == 3


def test_waiting_releases_the_gil(server, secret):
    info = new_match(server, secret, ["ai"])
    # One decision every five game seconds: the wait below really blocks.
    with aoeworld.connect(server, info.slots[0].token, decision_hz=0.2) as client:
        assert client.wait_decision(timeout=10) is not None
        counted = []
        stop = threading.Event()

        def count():
            total = 0
            while not stop.is_set():
                total += 1
            counted.append(total)

        thread = threading.Thread(target=count)
        thread.start()
        started = time.time()
        client.wait_decision(timeout=1.0)
        waited = time.time() - started
        stop.set()
        thread.join()
        assert waited >= 0.5
        # A thread holding the GIL through the wait would count almost nothing.
        assert counted[0] > 10_000, counted


def test_two_ai_players_grow_through_the_callback_loop(server, secret):
    info = new_match(server, secret, ["ai", "ai"])
    results = {}

    def play(slot):
        waypoints = []
        state = {"next": 0}

        def on_tick(client, decision):
            world = decision.world
            center = world.town_center()
            economy = world.economy
            if center is None or economy is None:
                return
            if not waypoints:
                cx, cy = center.center
                for radius in (10, 20, 30, 40):
                    for step in range(10):
                        angle = 6.2832 * step / 10
                        waypoints.append((cx + int(radius * math.cos(angle)), cy + int(radius * math.sin(angle))))
            for scout in world.idle_units(UnitKind.SCOUT):
                if state["next"] < len(waypoints):
                    client.move(scout.id, *waypoints[state["next"]])
                    state["next"] += 1
            for villager in world.idle_units(UnitKind.VILLAGER):
                node = world.nearest_resource(ResourceKind.WOOD, *villager.tile)
                if node is not None:
                    client.gather(villager.id, node.id)
            if center.queue_length == 0 and economy.food >= 50 and economy.free_population > 0:
                client.train(center.id, UnitKind.VILLAGER)

        with aoeworld.connect(server, slot.token, decision_hz=2) as client:
            summary = client.run(on_tick, until_tick=1500)
            results[slot.slot] = (summary, client.world)

    threads = [threading.Thread(target=play, args=(slot,)) for slot in info.slots]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(timeout=120)
    assert len(results) == 2
    for summary, world in results.values():
        assert summary.last_tick >= 1500 and not summary.closed
        assert len(world.my_units(UnitKind.VILLAGER)) > 3
        assert world.visible_enemies() == []


def test_a_fleet_drives_many_bots_from_one_thread(server, secret):
    info = new_match(server, secret, ["ai"] * 6)
    clients = [aoeworld.connect(server, slot.token, decision_hz=4) for slot in info.slots]
    try:
        fleet = aoeworld.Fleet(clients)
        assert len(fleet) == 6
        seen = [0] * 6
        deadline = time.time() + 20
        while time.time() < deadline and min(seen) < 3:
            for index, decision in fleet.wait(timeout=1.0):
                assert decision is not None
                seen[index] += 1
                assert decision.world.me == index
        assert min(seen) >= 3, seen
    finally:
        for client in clients:
            client.close()


def test_a_closed_connection_is_reported_once_and_never_busy_spins_the_fleet(server, secret):
    info = new_match(server, secret, ["ai"] * 2)
    clients = [aoeworld.connect(server, slot.token, decision_hz=4) for slot in info.slots]
    try:
        fleet = aoeworld.Fleet(clients)
        clients[0].close()
        reports = []
        deadline = time.time() + 20
        while time.time() < deadline and not any(i == 0 and d is None for i, d in reports):
            reports += fleet.wait(timeout=1.0)
        assert [i for i, d in reports if d is None] == [0], reports
        # Only the live bot is heard from now; a wait is never instant forever.
        started = time.time()
        later = []
        for _ in range(8):
            later += fleet.wait(timeout=0.25)
        assert all(i == 1 and d is not None for i, d in later), later
        clients[1].close()
        reports = []
        while time.time() < deadline and not reports:
            reports = fleet.wait(timeout=1.0)
        assert [(i, d) for i, d in reports] == [(1, None)]
        idle = time.time()
        for _ in range(4):
            assert fleet.wait(timeout=0.25) == []
        assert time.time() - idle >= 0.9, "waiting on a fully closed fleet must not spin"
    finally:
        for client in clients:
            client.close()
