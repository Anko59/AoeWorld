"""Planning and manager switches are available through the ordinary SDK."""
import aoeworld


def test_complete_technology_tree_and_market_quotes_are_public():
    rows = aoeworld.technologies()
    assert len(rows) == 53
    champion = next(row for row in rows if row.tech == aoeworld.Tech.CHAMPION)
    assert champion.prerequisite == aoeworld.Tech.TWO_HANDED_SWORDSMAN
    assert aoeworld.market_quote(100, True) == 130
    assert aoeworld.market_quote(100, False) == 70


def test_management_switches_are_public_client_orders():
    assert hasattr(aoeworld.Client, 'steward_settings')
    assert hasattr(aoeworld.Client, 'vassal_development')


def test_development_report_counts_managers_and_completed_fortifications():
    from aoe_ai.benchmark import measure
    from fakes import FakeWorld, base, building, unit
    units, buildings = base(3)
    buildings[0].garrison = [(900, aoeworld.UnitKind.STEWARD, 50, 50)]
    units.append(unit(901, aoeworld.UnitKind.GENERAL))
    buildings += [building(910, aoeworld.BuildingKind.CASTLE),
                  building(911, aoeworld.BuildingKind.WATCH_TOWER),
                  building(912, aoeworld.BuildingKind.WATCH_TOWER, complete=False)]
    row = measure(FakeWorld(units, buildings), kills=0, orders=0)
    assert row['management']['stewards'] == 1
    assert row['management']['generals'] == 1
    assert row['fortifications'] == {'castles': 1, 'towers': 1, 'wall_segments': 0}
