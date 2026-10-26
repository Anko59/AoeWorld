"""The Python SDK issues ordinary shelter orders against a real server."""
import time

import pytest
import aoeworld
from aoeworld import UnitKind


def test_garrison_and_selective_ejection_preserve_population(server, secret):
    info = aoeworld.start_match(server, secret, ["ai"], tiles_per_side=500, seed=3)
    with aoeworld.connect(server, info.slots[0].token, decision_hz=5) as client:
        first = client.wait_decision(timeout=10)
        center = first.world.town_center()
        worker = first.world.my_units(UnitKind.VILLAGER)[0]
        population = first.world.economy.population
        assert center.garrison_capacity == 15 and center.garrison == []
        client.garrison(worker.id, center.id)
        deadline = time.time() + 20
        while time.time() < deadline:
            decision = client.wait_decision(timeout=2)
            if decision and decision.world.town_center().garrison:
                current = decision.world
                break
        else:
            pytest.fail("villager did not enter the Town Center")
        assert current.town_center().garrison[0][0] == worker.id
        assert len(current.my_units(UnitKind.VILLAGER)) == 2
        assert current.economy.population == population
        client.ungarrison(center.id, worker.id)
        deadline = time.time() + 10
        while time.time() < deadline:
            decision = client.wait_decision(timeout=2)
            if decision and not decision.world.town_center().garrison:
                assert any(u.id == worker.id for u in decision.world.my_units(UnitKind.VILLAGER))
                assert decision.world.economy.population == population
                return
        pytest.fail("villager did not leave the Town Center")
