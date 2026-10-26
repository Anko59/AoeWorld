import time

import aoeworld


def test_formation_order_and_waypoint_are_authoritative(server, secret):
    info = aoeworld.start_match(server, secret, ["ai"], tiles_per_side=500, seed=3)
    with aoeworld.connect(server, info.slots[0].token, decision_hz=20) as client:
        first = client.wait_decision(timeout=10)
        scout = first.world.my_units(aoeworld.UnitKind.SCOUT)[0]
        x, y = scout.tile
        # Same land as the existing queued-move integration fixture.
        sequence = client.formation_move([scout.id], x + 12, y, aoeworld.Formation.BOX)
        client.formation_move([scout.id], x + 12, y + 8, aoeworld.Formation.BOX, queued=True)
        accepted = False
        deadline = time.time() + 10
        while time.time() < deadline:
            decision = client.wait_decision(timeout=2)
            if not decision:
                continue
            accepted |= any(e.kind == "order_result" and e.sequence == sequence
                            and e.result == aoeworld.CommandResult.ACCEPTED for e in decision.events)
            if len(decision.world.move_queue(scout.id)) == 2:
                assert accepted
                client.stop(scout.id)
                break
        else:
            raise AssertionError("Formation waypoints were not received")
        while time.time() < deadline:
            decision = client.wait_decision(timeout=2)
            if decision and not decision.world.move_queue(scout.id):
                return
        raise AssertionError("Stop did not clear the formation queue")
