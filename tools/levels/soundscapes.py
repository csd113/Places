#!/usr/bin/env python3
"""Idempotent audio dressing for existing maps; no geometry or visual changes."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def emitter(piece, sound, *, gain=1.0, looped=False, playing=False, time_domain="world"):
    components = piece.setdefault("components", [])
    components[:] = [value for value in components if value.get("component") != "audio"]
    components.append(dict(component="audio", sound=sound, gain=gain, looped=looped,
                           enabled=True, playing=playing, time_domain=time_domain))


def regional(level, identity, target, *, x, z, width, depth, bottom, top):
    volumes = level.setdefault("volumes", [])
    volumes[:] = [volume for volume in volumes if volume.get("id") != identity]
    volumes.append(dict(id=identity, x=x, z=z, width=width, depth=depth,
        bottom_y=bottom, top_y=top, filter=dict(kind="player"), occupancy="aggregate",
        bindings=[dict(on="enter_volume", actions=[dict(action="play_sound", target=target, loop=True)]),
                  dict(on="exit_volume", actions=[dict(action="stop_sound", target=target)])]))


def tap_mug(piece, identity):
    piece["id"] = identity
    piece["size"] = [.117, .09, .078]
    components = piece.setdefault("components", [])
    if not any(value.get("component") == "interactable" for value in components):
        components.append(dict(component="interactable", prompt="Tap mug", reach=1.8))
    emitter(piece, "prop_ceramic", gain=.75)
    bindings = piece.setdefault("bindings", [])
    bindings[:] = [value for value in bindings if value.get("id") != "audio_tap_mug"]
    bindings.append(dict(id="audio_tap_mug", on="interact", cooldown_seconds=.45,
                         actions=[dict(action="play_sound")]))


def apply_soundscape(level):
    identity = level.get("id")
    if identity == "office_demo":
        controller = next(piece for piece in level["props"] if piece.get("id") == "top_vending")
        # Spawn is in the upper vestibule. A seeded emitter avoids relying on an
        # enter event at load; it stops as the player descends below this storey.
        emitter(controller, "ambience_hvac", gain=.70, looped=True, playing=True, time_domain="real")
        regional(level, "audio_office_upper", "top_vending", x=-4, z=0,
                 width=40, depth=18, bottom=7.0, top=10.2)
        tap_mug(next(piece for piece in level["props"] if piece.get("id") == "top_mug"), "top_mug")
    elif identity == "pool_showcase":
        controller = next(piece for piece in level["props"] if piece.get("id") == "pool_water_demo_control")
        emitter(controller, "ambience_water", gain=.65, looped=True, playing=True)
        regional(level, "audio_pool_hall", "pool_water_demo_control", x=0, z=0,
                 width=16, depth=11, bottom=-1.6, top=4.5)
    elif identity == "home_showcase":
        mug = next(piece for piece in level["props"] if piece["model"] == "home:mug")
        tap_mug(mug, "home_audio_mug")
    elif identity == "frutiger_aero_demo":
        bench = next(piece for piece in level["props"] if piece.get("id") == "terrace_bench")
        emitter(bench, "ambience_outdoors", gain=.65, looped=True)
        regional(level, "audio_aero_garden", "terrace_bench", x=0, z=12,
                 width=32, depth=8, bottom=-.1, top=3.0)
    return level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    paths = args.paths or [ROOT / f"levels/{name}.json" for name in (
        "office_demo", "pool_showcase", "home_showcase", "frutiger_aero_demo")]
    for path in paths:
        old = json.loads(path.read_text())
        expected = apply_soundscape(json.loads(path.read_text()))
        if args.check:
            if expected != old:
                raise SystemExit(f"{path}: run tools/levels/soundscapes.py")
        else:
            path.write_text(json.dumps(expected, indent=2, ensure_ascii=False) + "\n")
    print(f"Audio dressing {'current' if args.check else 'written'}: {len(paths)} maps")


if __name__ == "__main__":
    main()
