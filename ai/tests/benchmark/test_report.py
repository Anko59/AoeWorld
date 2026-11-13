"""Long-run measurements keep actual samples and cannot certify truncated runs."""
from types import SimpleNamespace

import pytest

from aoe_ai.benchmark import Checkpoints, measure
from fakes import FakeWorld, base, economy, building
from aoeworld import BuildingKind, Tech


def test_samples_include_economy_research_and_distinct_settlement_cells():
    units, buildings = base(12)
    buildings += [building(501, BuildingKind.TOWN_CENTER, tile=(130, 50)),
                  building(502, BuildingKind.BARRACKS, tile=(134, 52))]
    eco = economy(food=1234, wood=5678, gold=321, stone=456)
    eco.researched = [Tech.LOOM, Tech.FEUDAL_AGE]
    world = FakeWorld(units, buildings, eco=eco)
    row = measure(world, kills=7, orders=19)
    assert row['population'] == eco.population
    assert row['town_centers'] == 2
    assert row['settled_cells'] == 2
    assert row['techs_researched'] == 2
    assert row['resources'] == dict(food=1234, wood=5678, gold=321, stone=456)
    assert row['kills'] == 7 and row['orders'] == 19


def test_a_late_observation_does_not_fabricate_missed_checkpoints():
    checkpoints = Checkpoints([30, 60, 120], tick_hz=20)
    assert checkpoints.due(30 * 1200) == 30
    checkpoints.record(30 * 1200, [{'population': 20}])
    with pytest.raises(ValueError, match='missed'):
        checkpoints.due(61 * 1200)


def test_final_checkpoint_is_recorded_once_and_completion_requires_all_times():
    checkpoints = Checkpoints([30, 60, 120], tick_hz=20)
    for minute in (30, 60, 120):
        tick = minute * 1200
        assert checkpoints.due(tick) == minute
        checkpoints.record(tick, [{'population': minute}])
        assert checkpoints.due(tick) is None
    assert checkpoints.complete
    assert [s['minute'] for s in checkpoints.samples] == [30, 60, 120]
    short = Checkpoints([30, 60, 120], tick_hz=20)
    assert not short.complete
    assert short.due(29 * 1200) is None


def test_foundations_do_not_count_as_developed_territory_or_finished_centers():
    units, buildings = base(3)
    buildings.append(building(501, BuildingKind.TOWN_CENTER, tile=(130, 50), complete=False))
    eco = economy()
    eco.researched = []
    row = measure(FakeWorld(units, buildings, eco=eco), kills=0, orders=0)
    assert row['town_centers'] == 1
    assert row['settled_cells'] == 1


def test_missed_or_empty_samples_cannot_claim_completion():
    checkpoints = Checkpoints([30, 60, 120], tick_hz=20)
    with pytest.raises(ValueError, match='empty'):
        checkpoints.record(30 * 1200, [])
    assert not checkpoints.complete


def test_missing_bot_has_a_wall_deadline_instead_of_endless_nudges(monkeypatch):
    from aoe_ai import arena
    now = iter(i * 0.06 for i in range(100))
    monkeypatch.setattr(arena.time, 'monotonic', lambda: next(now))
    monkeypatch.setattr(arena, 'DECISION_TIMEOUT_SECONDS', 0.1)
    fleet = SimpleNamespace(wait=lambda timeout: [])
    stepper = SimpleNamespace(step=lambda ticks, orders: ticks)
    with pytest.raises(TimeoutError, match='decision'):
        arena._await_decisions(fleet, {0}, stepper, 10)


def test_tick_stream_rejects_empty_missing_or_duplicate_timing_records():
    from aoe_ai.benchmark import TickStream
    stream = TickStream()
    with pytest.raises(ValueError, match='missing'):
        stream.add([], through_tick=10)
    stream.add([[1, 100, 80, 0, 1, 4, 0, 0], [2, 90, 70, 0, 0, 4, 0, 0]], through_tick=2)
    with pytest.raises(ValueError, match='gap'):
        stream.add([[4, 100, 80, 0, 0, 4, 0, 0]], through_tick=4)
    with pytest.raises(ValueError, match='gap'):
        stream.add([[2, 90, 70, 0, 0, 4, 0, 0]], through_tick=2)


def test_fortification_counts_separate_gates_and_retain_auditable_positions():
    units, buildings = base(3)
    buildings += [building(600, BuildingKind.WATCH_TOWER, tile=(60, 40)),
                  building(601, BuildingKind.STONE_WALL, tile=(70, 40)),
                  building(602, BuildingKind.STONE_GATE, tile=(71, 40)),
                  building(603, BuildingKind.CASTLE, tile=(80, 40)),
                  building(604, BuildingKind.WATCH_TOWER, tile=(90, 40), complete=False)]
    eco = economy()
    row = measure(FakeWorld(units, buildings, eco=eco), kills=0, orders=0)
    assert row['fortifications'] == dict(castles=1, towers=1, wall_segments=1, gates=1)
    assert {s['id'] for s in row['fortification_sites']} == {600, 601, 602, 603}
    assert next(s['tile'] for s in row['fortification_sites'] if s['id'] == 602) == list(buildings[-3].center)
