#!/usr/bin/env python3
"""Write twelve small native BLEND controls into a new, explicit directory.

This developer operation never bakes maps or runs in CI. Build its JSON sources
with places-compile --variants static,full into an isolated state root. It reuses
the preserved #24 control and existing assets; no images or models are generated.
"""
import argparse
import copy
import json
from pathlib import Path


def write_inputs(repository: Path, output: Path) -> None:
    fixtures = repository / "tests/fixtures/native/issue24-assets"
    base = json.loads((fixtures / "ghost-controls.json").read_text(encoding="utf8"))
    output.mkdir(parents=True, exist_ok=False)

    def save(name, document):
        document["id"] = document["name"] = name
        (output / f"{name}.json").write_text(
            json.dumps(document, indent=2) + "\n", encoding="utf8")

    for clip in ("idle", "float_forward"):
        document = copy.deepcopy(base)
        for prop in document["props"]:
            animation = next(component for component in prop["components"]
                             if component["component"] == "animation")
            animation["clip"] = clip
        save(f"ghost_{clip}", document)
        reflection = copy.deepcopy(document)
        reflection["defaults"]["floor"] = "core:pool_deck_wet_01"
        reflection["rooms"][0]["floor_shine"] = 1
        save(f"ghost_reflection_{clip}", reflection)
        if clip == "float_forward":
            moving = copy.deepcopy(reflection)
            moving["routes"] = [dict(id=prop["id"], loop=True, steps=[
                dict(step="move_to", x=prop["x"], z=prop["z"] + 1.8, speed=.3),
                dict(step="wait", seconds=5),
                dict(step="move_to", x=prop["x"], z=prop["z"], speed=.3),
                dict(step="wait", seconds=5)]) for prop in moving["props"]]
            save("ghost_reflection_float_moving", moving)
        interaction = copy.deepcopy(document)
        interaction["floor_patches"] = [dict(
            x=-9, z=-6, width=18, depth=12,
            material="core:pool_deck_wet_01", shine=1)]
        interaction["water"] = [dict(
            x=-3.5, z=2.5, width=3, depth=3, surface_y=.65, bottom_y=0,
            opacity=.4, swimming=False, motion=dict(speed=0, strength=0),
            caustics=dict(speed=0, strength=0))]
        interaction["walls"] = [dict(
            x=-3.7, z=4.65, width=3.4, depth=.04, height=2.4,
            openings=[dict(kind="window", offset=.1, width=3.2, height=2.2,
                           sill=.1, glass="core:glass_window_clear_01", solid=False)])]
        save(f"ghost_interactions_{clip}", interaction)

    for source in ("ghost-absent", "ghost-motion"):
        document = json.loads((fixtures / f"{source}.json").read_text(encoding="utf8"))
        save(document["id"], document)

    # The same accepted proximity fade/route, now on the existing wet deck.
    # The absent control proves zero fade leaves neither a reflection nor glow.
    motion = json.loads((fixtures / "ghost-motion.json").read_text(encoding="utf8"))
    motion["defaults"]["floor"] = "core:pool_deck_wet_01"
    motion["rooms"][0]["floor_shine"] = 1
    save("ghost_reflection_motion", motion)
    absent = copy.deepcopy(motion)
    absent["props"] = []
    absent["routes"] = []
    save("ghost_reflection_motion_absent", absent)

    # An explicit 32-actor workload, outside CI. Existing geometry and images
    # keep the measured delta specific to transparency submission and sorting.
    stress = copy.deepcopy(base)
    stress.pop("half_walls", None)
    stress["props"] = []
    for index in range(32):
        prop = copy.deepcopy(base["props"][2])
        prop.update(id=f"stress_{index}", x=(index % 8 - 3.5) * 1.4,
                    z=-(index // 8) * 1.4)
        stress["props"].append(prop)
    save("ghost_stress_32", stress)
    print(f"Wrote twelve native sources to {output}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path,
                        help="new directory for the twelve JSON sources")
    args = parser.parse_args()
    write_inputs(Path(__file__).resolve().parents[2], args.out.resolve())


if __name__ == "__main__":
    main()
