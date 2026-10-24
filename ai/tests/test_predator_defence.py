"""Predators are threats to workers, including before an army exists."""

import pytest
from aoeworld import AnimalKind, AnimalTask, UnitTask

from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.combat import CombatManager
from aoe_ai.play.scouting import ScoutManager
from fakes import FakeClient, FakeWorld, animal, base, decision


@pytest.mark.parametrize("kind", [AnimalKind.WOLF, AnimalKind.BOAR])
def test_workers_fight_a_predator_together_when_the_scout_is_away(kind):
    units, buildings = base(4)
    units[-1].tile = (200, 200)
    for worker in units[:-1]:
        worker.task, worker.idle = UnitTask.GATHERING, False
    wolf = animal(900, (50, 53), kind, food=0)
    wolf.task = AnimalTask.ATTACKING
    world = FakeWorld(units, buildings, animals=[wolf])
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    CombatManager().step(ctx)
    assert len(client.named("attack")) >= 3
    assert {a[1] for a in client.named("attack")} == {wolf.id}


def test_a_nearby_scout_protects_workers_without_a_scout_move_overwriting_it():
    units, buildings = base(3)
    wolf = animal(900, (50, 53), AnimalKind.WOLF, food=0)
    world = FakeWorld(units, buildings, animals=[wolf])
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    CombatManager().step(ctx)
    ScoutManager().step(ctx)
    assert client.named("attack") == [(units[-1].id, wolf.id)]
    assert not client.named("move")


def test_an_unprovoked_boar_is_left_alone():
    units, buildings = base(3)
    boar = animal(900, (50, 53), AnimalKind.BOAR)
    world = FakeWorld(units, buildings, animals=[boar])
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    CombatManager().step(ctx)
    assert client.calls == []


def test_workers_already_defending_are_reserved_without_repeating_their_orders():
    units, buildings = base(4)
    units[-1].tile = (200, 200)
    wolf = animal(900, (50, 53), AnimalKind.WOLF, food=0)
    for worker in units[:-1]:
        worker.task, worker.idle = UnitTask.ATTACKING, False
        worker.attack_target = wolf.id
    world = FakeWorld(units, buildings, animals=[wolf])
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    CombatManager().step(ctx)
    assert {u.id for u in units[:-1]} <= ctx.busy
    assert client.calls == []
