"""A real aoe-server on a free local port, shared by the whole test session."""

import os
import socket
import subprocess
import time
import urllib.request
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
SECRET = "pytest-match-secret-0123456789"


def _free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


@pytest.fixture(scope="session")
def server(tmp_path_factory):
    binary = Path(os.environ.get("AOE_SERVER_BIN", ROOT / "target" / "debug" / "aoe-server"))
    assert binary.exists(), f"build the server first: {binary}"
    port = _free_port()
    assert port not in (8080, 8081)
    packages = tmp_path_factory.mktemp("map-packages")
    env = dict(
        os.environ,
        AOE_BIND=f"127.0.0.1:{port}",
        AOE_MATCH_TOKEN=SECRET,
        AOE_TICK_HZ="60",
        AOE_MAP_PACKAGE_DIRECTORY=str(packages),
    )
    process = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    url = f"http://127.0.0.1:{port}"
    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            urllib.request.urlopen(f"{url}/health", timeout=1).read()
            break
        except OSError:
            assert process.poll() is None, "aoe-server exited during startup"
            time.sleep(0.2)
    else:
        process.kill()
        raise RuntimeError("aoe-server did not become healthy")
    yield url
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()


@pytest.fixture
def secret() -> str:
    return SECRET
