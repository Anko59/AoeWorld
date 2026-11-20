from aoe_ai.geometry import centroid, clamp_tile, dist, near_any, nearest, ring_points


def test_ring_points_lie_on_the_circle_and_get_denser_with_radius():
    small, large = ring_points((100, 100), 12), ring_points((100, 100), 60)
    assert len(large) > len(small) >= 8
    assert all(abs(dist(p, (100, 100)) - 60) < 1.5 for p in large)


def test_nearest_centroid_and_clamp():
    class P:
        def __init__(self, tile):
            self.tile = tile

    pts = [P((10, 10)), P((3, 3)), P((20, 1))]
    assert nearest(pts, (0, 0)).tile == (3, 3)
    assert nearest([], (0, 0)) is None
    assert centroid([(0, 0), (10, 20)]) == (5, 10)
    assert centroid([]) is None
    assert clamp_tile((-5, 400), (300, 300)) == (3, 296)


def test_near_any_answers_like_a_scan_of_every_tile():
    import random
    rng = random.Random(3)
    army = [(rng.randrange(400), rng.randrange(400)) for _ in range(60)]
    for radius in (0, 1, 7.5, 30):
        near = near_any(army, radius)
        for _ in range(500):
            tile = (rng.randrange(-40, 440), rng.randrange(-40, 440))
            assert near(tile) == any(dist(tile, u) <= radius for u in army)
    assert near_any([], 30)((5, 5)) is False
