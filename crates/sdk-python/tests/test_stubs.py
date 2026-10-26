"""The .pyi stubs must describe exactly what the native module exports."""

import ast
from pathlib import Path

import aoeworld
from aoeworld import _native

STUB = Path(aoeworld.__file__).with_name("_native.pyi")


def test_every_stubbed_name_exists_and_every_export_is_stubbed():
    tree = ast.parse(STUB.read_text())
    stubbed = {
        node.name: node
        for node in tree.body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef))
    }
    exported = {name for name in dir(_native) if not name.startswith("_")}
    assert set(stubbed) == exported, (set(stubbed) ^ exported)
    for name, node in stubbed.items():
        if isinstance(node, ast.ClassDef):
            cls = getattr(_native, name)
            for member in node.body:
                if isinstance(member, ast.FunctionDef) and not member.name.startswith("__"):
                    assert hasattr(cls, member.name), f"{name}.{member.name}"
                if isinstance(member, ast.AnnAssign) and name[0].isupper():
                    assert hasattr(cls, member.target.id), f"{name}.{member.target.id}"


def test_package_reexports_the_native_api():
    for name in aoeworld.__all__:
        assert hasattr(aoeworld, name), name
    assert aoeworld.UnitKind.VILLAGER != aoeworld.UnitKind.SCOUT
    assert aoeworld.BuildingKind.TOWN_CENTER == aoeworld.BuildingKind.TOWN_CENTER
