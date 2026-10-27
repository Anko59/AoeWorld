"""Stances use the ordinary command queue and authoritative unit records."""
import time

import aoeworld


def test_stance_is_acknowledged_and_observed_on_an_ai_owned_unit(server, secret):
    info = aoeworld.start_match(server, secret, ["ai"], tiles_per_side=500, seed=3)
    with aoeworld.connect(server, info.slots[0].token, decision_hz=20) as client:
        first = client.wait_decision(timeout=10)
        scout = first.world.my_units(aoeworld.UnitKind.SCOUT)[0]
        sequence = client.set_stance([scout.id], aoeworld.CombatStance.NO_ATTACK)
        accepted = False
        deadline = time.time() + 10
        while time.time() < deadline:
            decision = client.wait_decision(timeout=2)
            if not decision:
                continue
            accepted |= any(e.kind == "order_result" and e.sequence == sequence
                            and e.result == aoeworld.CommandResult.ACCEPTED for e in decision.events)
            unit = next(u for u in decision.world.my_units() if u.id == scout.id)
            if accepted and unit.stance == aoeworld.CombatStance.NO_ATTACK:
                return
        raise AssertionError("The AI slot never observed its accepted stance")
