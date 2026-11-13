"""Observer-only kill attribution; none of these admin facts enter the Brain."""
from ..geometry import dist


def projection_sample(board, player, kills=None):
    records = list(board.projection_records)
    if board.expedition is not None:
        exp = board.expedition
        records.append(dict(target=exp.target,crew=exp.crew,origin=exp.origin,goal=exp.goal,
                            crossing=exp.crossed,proof=exp.proof))
    attacks = []
    for kill in kills if kills is not None else [player['last_building_destroyed']]:
        if kill is None:
            continue
        for record in records:
            proof = record['proof']
            if (record['target'] == kill['target'] and kill['attacker'] in record['crew']
                    and record['crossing'] in ('water', 'bridge') and proof
                    and kill['tick'] >= proof.get('tick', 0)
                    and dist(record['origin'], kill['tile']) >= 80
                    and all(dist(center, kill['tile']) >= 80 for center in kill.get('own_centers', []))):
                attacks.append(dict(kill=kill, crossing=record['crossing'], proof=proof,
                                    origin=record['origin'], goal=record['goal']))
                break
    return dict(enemy_buildings_destroyed=player['enemy_buildings_destroyed'],
                unit_losses=player['unit_losses'],
                bridges_built=board.stats.get('bridges_built', 0),
                transports_used=board.stats.get('transports_used', 0),
                forward_buildings_built=board.stats.get('forward_buildings_built', 0),
                qualified_attacks=attacks)
