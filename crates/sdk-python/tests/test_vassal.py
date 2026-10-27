"""The new original unit shares authoritative rules through the Python SDK."""
import aoeworld


def test_vassal_castle_cost_and_training_rules():
    assert aoeworld.trainable(aoeworld.BuildingKind.CASTLE) == [aoeworld.UnitKind.VASSAL]
    cost = aoeworld.unit_cost(aoeworld.UnitKind.VASSAL)
    assert cost == (1000, 0, 1000, 0)
    assert aoeworld.train_seconds(aoeworld.UnitKind.VASSAL) == 60
