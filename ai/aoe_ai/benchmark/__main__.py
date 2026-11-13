"""Run the long-game workload through the Dockerized Make entry point."""
import argparse
from pathlib import Path

from .runner import run

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--server-bin', type=Path, required=True)
parser.add_argument('--map-hash', required=True)
parser.add_argument('--bots', type=int, required=True)
parser.add_argument('--seed', type=int, required=True)
parser.add_argument('--starts', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--check-starts', action='store_true')
args = parser.parse_args()
run(args)
