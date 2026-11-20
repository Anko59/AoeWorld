"""Small tile-geometry helpers shared by the managers."""

from __future__ import annotations

import math
from typing import Iterable, Optional, Sequence, TypeVar

T = TypeVar("T")
Tile = tuple[int, int]


def dist2(a: Tile, b: Tile) -> int:
    return (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2


def dist(a: Tile, b: Tile) -> float:
    return math.sqrt(dist2(a, b))


def nearest(items: Iterable[T], origin: Tile, key=lambda item: item.tile) -> Optional[T]:
    best: Optional[T] = None
    best_d = None
    for item in items:
        d = dist2(key(item), origin)
        if best_d is None or d < best_d:
            best, best_d = item, d
    return best


def centroid(tiles: Sequence[Tile]) -> Optional[Tile]:
    if not tiles:
        return None
    return (sum(t[0] for t in tiles) // len(tiles), sum(t[1] for t in tiles) // len(tiles))


def ring_points(center: Tile, radius: int, spacing: int = 11) -> list[Tile]:
    """Points on a circle, about ``spacing`` tiles apart (at least eight)."""
    count = max(8, int(2 * math.pi * radius / spacing))
    return [
        (
            center[0] + int(round(radius * math.cos(2 * math.pi * i / count))),
            center[1] + int(round(radius * math.sin(2 * math.pi * i / count))),
        )
        for i in range(count)
    ]


def clamp_tile(tile: Tile, size: tuple[int, int], margin: int = 3) -> Tile:
    return (
        min(max(tile[0], margin), size[0] - 1 - margin),
        min(max(tile[1], margin), size[1] - 1 - margin),
    )


def near_any(tiles: Iterable[Tile], radius: float):
    """A test of whether a tile lies within `radius` of any of `tiles`, the
    same answer as `any(dist(t, u) <= radius for u in tiles)`, from a grid of
    `radius`-sized cells: only the 3x3 cells around a tile can hold one."""
    size = max(1, math.ceil(radius))
    cells: dict[Tile, list[Tile]] = {}
    for u in tiles:
        cells.setdefault((u[0] // size, u[1] // size), []).append(u)

    def test(tile: Tile) -> bool:
        cx, cy = tile[0] // size, tile[1] // size
        return any(dist(tile, u) <= radius
                   for dx in (-1, 0, 1) for dy in (-1, 0, 1)
                   for u in cells.get((cx + dx, cy + dy), ()))
    return test
