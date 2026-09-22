"""Prop part modules.

Each module exposes ``PROPS = {"core:<id>": build_function}``.  A build
function takes a :class:`mesh.PropBuilder` and is responsible for painting the
texture and pushing primitives; see ``parts/utility.py`` for the commented
exemplar and ``tools/props/README.md`` for the asset rules.
"""

from __future__ import annotations

from typing import Callable, Dict

from mesh import PropBuilder

BuildFn = Callable[[PropBuilder], None]

MODULES = ("furniture", "appliances", "utility", "decor", "spooner_man")


def collect() -> Dict[str, BuildFn]:
    """Imports every part module and merges their registries."""
    registry: Dict[str, BuildFn] = {}
    for module_name in MODULES:
        try:
            module = __import__(f"parts.{module_name}", fromlist=["PROPS"])
        except ImportError as error:  # pragma: no cover - developer feedback
            raise ImportError(
                f"prop part module parts/{module_name}.py is missing ({error}); "
                "the pack expects the furniture, appliances, utility, decor and "
                "spooner_man modules"
            ) from error
        props = getattr(module, "PROPS", {})
        for prop_id, build in props.items():
            if prop_id in registry:
                raise ValueError(f"{prop_id} is registered in more than one part module")
            registry[prop_id] = build
    return registry
