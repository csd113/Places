#!/usr/bin/env python3
"""Run every movement_test station through the game's existing capture controls.

Build the release binary and compile the map first. Evidence belongs under target/;
these are scripted real SDL/wgpu gameplay runs, separate from deterministic tests.
"""
import argparse
import csv
import json
import os
import shutil
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def cases():
    result = []

    def add(name, station, spawn, script, seconds, interact=None):
        result.append(dict(name=name, station=station, spawn=spawn, script=script,
                           seconds=seconds, interact=interact or f"station_{station:02}"))

    for name, script in [("forward", "forward@0-1"), ("backward", "backward@0-1"),
                         ("strafe", "strafe_right@0-1"),
                         ("diagonal", "forward@0-1,strafe_right@0-1")]:
        add(f"flat_{name}", 1, "15,10,90", script, 1)
    add("wall_slide", 1, "10,14,90", "forward@0-2,strafe_right@0-2", 2)
    add("narrow_lane", 1, "28.6,13,180", "forward@0-2", 2)
    add("oblique_corner", 1, "6.4,21,0", "forward@0-2", 2)
    for z, h in [(5.7, .1), (7.7, .3), (9.7, .39), (11.7, .4), (13.7, .41), (15.7, .6)]:
        add(f"step_{h:.2f}", 2, f"38,{z},90", "forward@0-1.2", 1.2)
    add("header_standing", 2, "48,8,90", "forward@0-1", 1)
    add("header_crouch", 2, "48,8,90", "crouch@0-0.05,forward@0.1-1.2", 1.2)
    for name, x, top, height in [("narrow", 4.4, 35, .9), ("normal", 7.7, 35, 1.5),
                                  ("steep", 10.6, 31.5, 1.6), ("wide", 14.5, 35, 1.5),
                                  ("door", 18.5, 33, 1.2)]:
        duration = (top - 27.5) / 3
        add(f"stairs_{name}_up", 3, f"{x},27.5,180", f"forward@0-{duration}", duration)
        add(f"stairs_{name}_down", 3, f"{x},{top},0", f"forward@0-{duration}", duration)
    for name, x, top in [("gentle", 28.6, 34.5), ("moderate", 31.6, 34.5),
                         ("steep", 34.6, 31.5), ("near_limit", 37.4, 30.5),
                         ("limit_narrow", 40.4, 30.5)]:
        duration = (top - 27.5) / 3
        add(f"ramp_{name}", 4, f"{x},27.5,180", f"forward@0-{duration}", duration)
    add("ramp_peak", 4, "27.5,37.5,90", "forward@0-2.4", 2.4)
    add("ramp_valley", 4, "36.2,37.5,90", "forward@0-2", 2)
    for z, h in [(29, .6), (34, 1.5), (39, 3)]:
        add(f"ledge_{h}", 5, f"56,{z},90", "forward@0-2", 2)
    add("unsupported_edge", 5, "70,35,90", "forward@0-1.6", 1.6)
    add("unsupported_recovery", 5, "70,35,90", "forward@0-3", 3)
    for name, x in [("low", 8), ("borderline", 12), ("too_high", 16), ("narrow", 19.3)]:
        add(f"jump_{name}", 6, f"{x},49.5,180", "jump@0-0.06,forward@0-0.7", 1.6)
    add("jump_desk_apex", 6, "7,57.5,180", "jump@0-0.06,forward@0-0.55", .4)
    add("jump_desk_land", 6, "7,57.5,180", "jump@0-0.06,forward@0-0.55", 1.6)
    for direction in ["backward", "strafe_left", "strafe_right"]:
        add(f"jump_{direction}", 6, "4,55,0", f"jump@0-0.06,{direction}@0-0.6", 1.3)
    add("jump_stair_transition", 3, "7.7,27.5,180", "forward@0-1.3,jump@0.25-0.31", 1.3)
    add("jump_wall", 6, "10.6,59,90", "jump@0-0.06,forward@0-1", 1)
    add("crouch_passage", 7, "37.5,48,180", "crouch@0-0.05,forward@0.1-1.5", 1.5)
    add("crouch_forced", 7, "37.5,48,180", "crouch@0-0.05,forward@0.1-1,crouch@1.1-1.15", 1.5)
    add("crouch_reopen", 7, "37.5,48,180", "crouch@0-0.05,forward@0.1-1,backward@1.1-2.1,crouch@2.2-2.25", 2.8)
    add("rapid_stance", 7, "32,48,0", "crouch@0-0.05,crouch@0.1-0.15,crouch@0.2-0.25,crouch@0.3-0.35", .8)
    add("tight_shaft", 7, "43.6,51,0", "jump@0-0.06,forward@0-0.5", 1.3)
    add("head_jump", 7, "41,60,0", "jump@0-0.06", .2)
    add("door_jump", 8, "58.9,48,180", "jump@0-0.06,forward@0-2.5", 2.5)
    add("door_narrow", 8, "52.35,48,180", "forward@0-1.5", 1.5)
    add("door_threshold", 8, "64.6,48,180", "forward@0-1.2", 1.2)
    add("door_closed", 8, "52.6,56.5,180", "forward@0-1", 1, "station_08")
    add("door_interactive_open", 8, "52.6,55,180", "forward@1-2.5", 2.5, "station_08,closed_door")
    add("door_open_leaf", 8, "58.6,56,180", "forward@0-1.5", 1.5)
    add("door_ramp", 8, "64.6,53.5,180", "forward@0-2", 2)
    add("pool_entry", 9, "10,71,180", "forward@0-2.2", 2.2)
    add("pool_surface", 9, "10,75,0", "jump@0-4", 4)
    add("pool_flat_exit", 9, "10,74,0", "jump@0-6,forward@4-6", 6)
    add("pool_raised_exit", 9, "17,75,90", "jump@0-6,forward@4-6", 6)
    add("pool_wall_refused", 9, "16,79,180", "jump@0-7,forward@4-7", 7)
    add("pool_stairs_exit", 9, "13.75,76.5,180", "jump@0-8,forward@4-8", 8)
    add("pool_shallow_entry", 9, "3.5,75,90", "forward@0-1", 1)
    for name, x in [("desk", 30), ("chair", 34), ("crate", 38), ("narrow", 42)]:
        add(f"prop_{name}", 10, f"{x},72,180", "forward@0-2", 2)
    add("prop_large", 10, "30,79,180", "forward@0-2", 2)
    add("prop_round", 10, "36,81,90", "forward@0-2", 2)
    add("opposing_walls", 11, "55,73.6,90", "forward@0-2", 2)
    add("pillar_wall", 11, "58.5,72,180", "forward@0-2", 2)
    add("wedge", 11, "64.5,77,0", "forward@0-2", 2)
    add("stair_head_door", 11, "52.6,77.5,180", "forward@0-2", 2)
    add("ramp_wall", 11, "56.6,78.5,180", "forward@0-1.3", 1.3)
    add("prop_door", 11, "63,79.5,180", "forward@0-2,jump@0.2-0.26", 2)
    add("sloped_head", 12, "75,55,90", "jump@0-0.06,forward@0-2", 2)
    result.append(dict(name="pit_stacked_balcony", station=0,
        spawn="50.2,1.6,-29.2,180", script="jump@0-0.06,forward@0-0.58",
        seconds=1.5, interact="", level="level0_pit"))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/movement-qa/captures")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--quality", choices=["low", "high"], default="high")
    parser.add_argument("--case", default="", help="only case names containing this text")
    parser.add_argument("--pit-package", type=Path, default=ROOT / "levels/level0_pit.placesmap",
                        help="compiled Pit package for the balcony regression")
    args = parser.parse_args()
    out = args.out.resolve() / args.quality
    out.mkdir(parents=True, exist_ok=True)
    state = out / "state"
    state.mkdir(exist_ok=True)
    (state / "settings.json").write_text(json.dumps(dict(bindings=dict(forward="W", backward="S",
        strafe_left="A", strafe_right="D", look_up="UP", look_down="DOWN",
        look_left="LEFT", look_right="RIGHT"), walk_speed=3, vsync=False,
        quality=args.quality, texture_filtering=args.quality, reflections="off",
        lightmaps="off" if args.quality == "low" else "full", bloom=False,
        window_width=640, window_height=360, window_mode="windowed")))
    selected = [case for case in cases() if args.case in case["name"]]
    if any(case.get("level") == "level0_pit" for case in selected):
        if not args.pit_package.is_file():
            parser.error("compile tests/fixtures/levels/level0_pit.json and supply --pit-package")
        installed = state / "levels"
        installed.mkdir(exist_ok=True)
        shutil.copy2(args.pit_package, installed / "level0_pit.placesmap")
    records = []
    for case in selected:
        if args.case not in case["name"]:
            continue
        name = case["name"]
        csv_path = out / f"{name}.csv"
        png_path = out / f"{name}.png"
        csv_path.unlink(missing_ok=True)
        png_path.unlink(missing_ok=True)
        env = dict(os.environ, PLACES_STATE_ROOT=str(state), PLACES_ASSET_ROOT=str(ROOT),
            PLACES_LEVEL=case.get("level", "movement_test"), PLACES_QUALITY=args.quality, PLACES_BENCH="1",
            PLACES_SPAWN=case["spawn"], PLACES_CAMERA=case["spawn"].split(",")[-1] + ",-20",
            PLACES_MOVE_SCRIPT=case["script"], PLACES_INTERACT=case["interact"],
            PLACES_CAPTURE_TIME=str(case["seconds"]), PLACES_CAPTURE=str(png_path),
            PLACES_STATE_LOG=str(csv_path), PLACES_VERBOSE="1")
        try:
            with (out / f"{name}.log").open("w") as log:
                completed = subprocess.run([str(args.binary.resolve())], cwd=ROOT, env=env,
                    stdout=log, stderr=subprocess.STDOUT, timeout=120, check=False)
            record = dict(case, exit_code=completed.returncode)
            if completed.returncode == 0 and not png_path.is_file():
                record["exit_code"] = "missing capture"
            if csv_path.exists():
                with csv_path.open() as source:
                    rows = list(csv.reader(source))
                record["final_state"] = rows[-1] if rows else None
            captured_log = (out / f"{name}.log").read_text()
            if "is not a valid settings file" in captured_log or "PLACES_MOVE_SCRIPT: ignored" in captured_log:
                record["exit_code"] = "invalid capture setup"
            if record.get("final_state") is None:
                record["exit_code"] = "missing trajectory"
            records.append(record)
            print(f"{name}: exit {record['exit_code']}", flush=True)
        except subprocess.TimeoutExpired:
            records.append(dict(case, exit_code="timeout"))
            print(f"{name}: timeout", flush=True)
    (out / "manifest.json").write_text(json.dumps(records, indent=2) + "\n")
    failed = sum(record["exit_code"] != 0 for record in records)
    if not records:
        parser.error("no capture cases matched")
    print(f"{len(records)} captures, {failed} failures: {out}")
    return int(bool(failed))


if __name__ == "__main__":
    raise SystemExit(main())
