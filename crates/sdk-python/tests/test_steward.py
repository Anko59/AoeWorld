"""The new original unit shares authoritative rules through the Python SDK."""
import aoeworld


def test_steward_town_center_cost_and_training_rules():
    assert aoeworld.trainable(aoeworld.BuildingKind.TOWN_CENTER) == [aoeworld.UnitKind.VILLAGER, aoeworld.UnitKind.STEWARD]
    assert aoeworld.trainable(aoeworld.BuildingKind.CASTLE) == [aoeworld.UnitKind.VASSAL, aoeworld.UnitKind.GENERAL]
    cost = aoeworld.unit_cost(aoeworld.UnitKind.STEWARD)
    assert cost == (1000, 0, 1000, 0)
    assert aoeworld.train_seconds(aoeworld.UnitKind.STEWARD) == 60


def test_general_castle_cost_and_training_rules():
    assert aoeworld.UnitKind.GENERAL in aoeworld.trainable(aoeworld.BuildingKind.CASTLE)
    assert aoeworld.unit_cost(aoeworld.UnitKind.GENERAL) == (800, 0, 800, 0)
    assert aoeworld.train_seconds(aoeworld.UnitKind.GENERAL) == 60
