"""Regression tests for review findings: hit proof, refused approach tiles,
the per-decision job cap and abandoned foundations."""

from aoeworld import BuildingKind, CommandResult, UnitKind, UnitTask

from aoe_ai import Brain
from aoe_ai.play.economy import MAX_ASSIGN_PER_DECISION

from fakes import FakeClient, FakeWorld, base, building, decision, economy, order_result, unit
from test_logic import play


def armed(count=12, tile=(60, 60)):
    return [unit(300 + i, UnitKind.SPEARMAN, tile=tile) for i in range(count)]


def test_a_foundation_rising_in_health_is_not_a_landed_blow():
    units, buildings = base(10)
    foundation = building(901, BuildingKind.HOUSE, tile=(64, 64), complete=False, mine=False, hp=100)
    brain, _ = play(FakeWorld(units + armed(), buildings, enemy_buildings=[foundation]))
    foundation.hp = 400
    play(FakeWorld(units + armed(), buildings, enemy_buildings=[foundation]), brain=brain)
    assert brain.board.attacking
    assert brain.board.stats.get("attack_hits", 0) == 0


def test_a_blow_counts_only_when_health_falls_with_our_soldiers_near():
    units, buildings = base(10)
    target = building(901, BuildingKind.BARRACKS, tile=(64, 64), mine=False, hp=1000)
    brain, _ = play(FakeWorld(units + armed(), buildings, enemy_buildings=[target]))
    target.hp = 900
    play(FakeWorld(units + armed(), buildings, enemy_buildings=[target]), brain=brain)
    assert brain.board.stats["attack_hits"] == 1
    far = building(902, BuildingKind.BARRACKS, tile=(250, 250), mine=False, hp=1000)
    brain, _ = play(FakeWorld(units + armed(), buildings, enemy_buildings=[far]))
    far.hp = 900
    play(FakeWorld(units + armed(), buildings, enemy_buildings=[far]), brain=brain)
    assert brain.board.stats.get("attack_hits", 0) == 0


def test_a_refused_approach_tile_is_not_sent_again():
    units, buildings = base(10)
    target = building(901, BuildingKind.BARRACKS, tile=(150, 120), mine=False, visible=False)
    world = lambda: FakeWorld(units + armed(), buildings, enemy_buildings=[target])  # noqa: E731
    brain, client = play(world())
    first = next(a for a in client.named("move") if a[0] >= 300)
    sequence = next(i + 1 for i, (n, a) in enumerate(client.calls) if n == "move" and a[0] >= 300)
    refusal = order_result(sequence, CommandResult.REJECTED_INVALID_DESTINATION)
    _, client = play(world(), brain=brain, events=[refusal])
    second = next(a for a in client.named("move") if a[0] >= 300)
    assert (first[1], first[2]) != (second[1], second[2])


def test_villagers_over_the_decision_cap_wait_unstamped_and_dead_ones_are_forgotten():
    units, buildings = base(MAX_ASSIGN_PER_DECISION + 4)
    brain = Brain()
    brain.board.assigned[9999] = 1  # a villager that died long ago
    client = FakeClient()
    from fakes import node
    from aoeworld import ResourceKind
    nodes = [node(i, ResourceKind.WOOD, (55 + i % 5, 55)) for i in range(30)]
    world = FakeWorld(units, buildings, nodes=nodes, eco=economy(population=20, cap=40))
    brain.decide(client, decision(world))
    stamped = {i for i in brain.board.assigned if i in {u.id for u in units}}
    assert 9999 not in brain.board.assigned
    assert len(stamped) == MAX_ASSIGN_PER_DECISION


def test_a_stalled_foundation_gets_a_villager_sent_to_construct_it():
    units, buildings = base(10)
    for worker in units:
        worker.idle, worker.task = False, UnitTask.GATHERING
    foundation = building(700, BuildingKind.BARRACKS, tile=(56, 56), complete=False)
    foundation.progress = 50
    brain, _ = play(FakeWorld(units, buildings + [foundation], tick=100))
    later = FakeWorld(units, buildings + [foundation], tick=100 + 20 * 9)
    _, client = play(later, brain=brain)
    assert [a for a in client.named("construct") if a[1] == 700]
