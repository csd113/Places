#!/usr/bin/env python3
"""Audit explicit environment native evidence. No bake, launches, or CI hooks.

Declarative assertions target snapshot paths (dot-separated dict/list keys;
`characters[id=caretaker].visible` selects one uniquely identified list record).
Supported operators: eq/ne, gt/ge/lt/le, contains, exists, near, xz_near,
range, all_eq, all_match. Missing data always fails, including evidence of full-model
occlusion: absence of a field must never be interpreted as invisibility.
"""
from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path
import re
import statistics

from environment_acceptance import ORDINARY, write


def lookup(value, path: str):
    for key in path.split(".") if path else []:
        selector = re.fullmatch(r"([^\[]+)\[([^=\]]+)=([^\]]+)\]", key)
        if selector:
            name, field, wanted = selector.groups()
            def selected(row):
                if isinstance(row, dict):
                    return row.get(field)
                if isinstance(row, list) and field.isdecimal() and int(field) < len(row):
                    return row[int(field)]
                return None
            matches = [row for row in value[name] if str(selected(row)) == wanted]
            if len(matches) != 1:
                raise KeyError(f"expected one {key}, found {len(matches)}")
            value = matches[0]
        else:
            value = value[int(key)] if isinstance(value, list) else value[key]
    return value


def assertion_matches(actual, assertion: dict) -> bool:
    operator, expected = assertion["op"], assertion.get("value")
    if operator == "exists":
        return True
    if operator == "eq":
        return actual == expected
    if operator == "ne":
        return actual != expected
    if operator == "contains":
        return expected in actual
    if operator == "range":
        return expected[0] <= actual <= expected[1]
    if operator == "all_eq":
        return all(value == expected for value in actual)
    if operator == "all_match":
        return all(all(lookup(value, key) == wanted for key, wanted in expected.items()) for value in actual)
    if operator == "near":
        return math.dist(actual, expected) <= assertion.get("tolerance", .1) if isinstance(actual, list) else abs(actual-expected) <= assertion.get("tolerance", .1)
    if operator == "angle_near":
        return abs((actual-expected+math.pi) % math.tau-math.pi) <= assertion.get("tolerance", .015)
    if operator == "xz_near":
        return math.dist([actual[0], actual[2]], expected) <= assertion.get("tolerance", .1)
    if operator == "gt":
        return actual > expected
    if operator == "ge":
        return actual >= expected
    if operator == "lt":
        return actual < expected
    if operator == "le":
        return actual <= expected
    raise ValueError(f"unknown assertion operator: {operator}")


def performance(folder: Path, receipt: dict) -> dict:
    log = (folder / "native.log").read_text()
    env = json.loads((folder / "environment.json").read_text())
    with (folder / "frames.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    loops = [float(row["loop_ms"]) for row in rows]
    modes = re.findall(r"\[wgpu\] present mode ([^\s]+)", log)
    if not modes:
        modes = re.findall(r"present mode: ([^\s|]+)", log)
    drawables = [list(map(int, match)) for match in re.findall(r"drawable[: ]+(\d+)x(\d+)", log)]
    summaries = [json.loads(line.split(" ", 1)[1]) for line in log.splitlines() if line.startswith("BENCH_SUMMARY ")]
    refresh = receipt.get("requested_refresh_hz")
    display = folder / "display.json"
    refresh_source = "explicit independently verified refresh" if refresh else None
    if display.exists():
        display_data = json.loads(display.read_text())
        active = [screen for gpu in display_data.get("SPDisplaysDataType", [])
                  for screen in gpu.get("spdisplays_ndrvs", [])
                  if screen.get("spdisplays_main") == "spdisplays_yes"]
        if len(active) == 1:
            rates = re.findall(r"@\s*([\d.]+)Hz", active[0].get("_spdisplays_resolution", ""))
            if rates:
                refresh = float(rates[0])
                refresh_source = "system_profiler active main display"
    mean_ms = statistics.mean(loops) if loops else None
    average_fps = 1000/mean_ms if mean_ms else None
    faster_fraction = sum(value < 1000/refresh*.95 for value in loops)/len(loops) if loops and refresh else None
    uncapped = bool(average_fps and refresh and average_fps > refresh*1.1
                    and faster_fraction is not None and faster_fraction > .5
                    and modes and modes[-1].lower() == "immediate")
    events = [json.loads(line) for line in (folder / "trace.jsonl").read_text().splitlines()]
    skipped = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
    requested_off = env.get("PLACES_VSYNC") == "off"
    pacing_env = [key for key in env if "VIDEO" in key or key in {"PLACES_BENCH_FIXED_DELTA_SECONDS", "PLACES_BENCH_FINISH", "PLACES_BENCH_NOSWAP", "PLACES_BENCH_NORENDER"}]
    committed = re.findall(r"\[loading\] committed ([^\s]+)", log)
    return dict(measured_frames=len(rows), expected_frames=int(env.get("PLACES_BENCH_FRAMES", 0)),
                all_frames_have_scene_geometry=bool(rows) and all(int(row["draw_calls"]) > 0 for row in rows),
                mean_loop_ms=mean_ms, average_fps=average_fps,
                median_loop_ms=statistics.median(loops) if loops else None,
                p95_loop_ms=sorted(loops)[max(0, math.ceil(len(loops)*.95)-1)] if loops else None,
                actual_presentation_modes=modes, physical_drawables=drawables,
                actual_refresh_hz=refresh, refresh_source=refresh_source,
                fraction_faster_than_refresh=faster_fraction, requested_vsync_off=requested_off,
                pacing_overrides=pacing_env, surface_not_presented_ready=skipped,
                verified_uncapped=uncapped and requested_off and not pacing_env and not skipped,
                throughput_status="uncapped verified" if uncapped and requested_off and not pacing_env and not skipped else "inconclusive: uncapped presentation/refresh throughput not established",
                selected_level=receipt.get("level"), observed_level_commits=committed,
                only_selected_level_committed=committed == [receipt.get("level")],
                native_memory=receipt.get("native_memory"),
                benchmark_summary=summaries[-1] if summaries else None)


def audit(folder: Path, expectations: Path | None = None, require_uncapped: bool = False) -> dict:
    samples = json.loads((folder / "snapshots.json").read_text())
    receipt = json.loads((folder / "receipt.json").read_text())
    steps = json.loads((folder / "actions.json").read_text())
    settings = json.loads((folder / "settings.json").read_text())
    expected_path = expectations or folder / "expectations.json"
    expected = json.loads(expected_path.read_text()) if expected_path.exists() else {}
    events = [json.loads(line) for line in (folder / "trace.jsonl").read_text().splitlines()]
    log = (folder / "native.log").read_text()
    checks = []
    def require(ok, label, detail=None):
        checks.append(dict(check=label, passed=bool(ok), **({"detail": detail} if detail is not None else {})))
    require(receipt.get("exit_code") == 0 and receipt.get("shutdown"), "clean direct native shutdown")
    require(receipt.get("immutable_inputs"), "binary and supplied package unchanged during run")
    if receipt.get("import_skipped"):
        custody = receipt.get("bundled_custody") or {}
        require(custody.get("real_file") and custody.get("sha256") == receipt.get("package_sha256"),
                "import skipped only with matching real bundled package custody", custody.get("file"))
    natural = expected.get("ordinary", not receipt.get("diagnostic_setup", False))
    if natural:
        require(all(step["action"]["kind"] in ORDINARY for step in steps), "ordinary held input/camera only; no direct level/pose/item/goal/flag activation")
    if not receipt.get("performance") and not receipt.get("smooth_video"):
        require(bool(samples), "native captures exist")
    else:
        require(receipt.get("focused_action_results", 0) > 0, "actual native focus observed during timing/video setup")
    for name, sample in samples.items():
        require(sample.get("window_focused") and sample.get("window_native_focus"), name+": actual SDL/native window focus")
        require(sample.get("current_level_id") == sample.get("renderer_level_id"), name+": renderer and runtime map agree")
        if settings.get("lightmaps") == "full" and sample.get("current_level_id"):
            require(sample.get("renderer_lightmaps") == "full" and sample.get("renderer_lighting") == "high",
                    name+": requested Full lighting is actually applied",
                    {key: sample.get(key) for key in ("renderer_lightmaps", "renderer_lighting")})
        require(not sample.get("noclip", False), name+": physical movement enabled")
        stats = sample.get("physics", {}).get("stats", {})
        require(stats.get("recovered_bodies") == 0 and stats.get("suspended_bodies") == 0, name+": no recovered or suspended physics bodies")
        require(not any(re.search(r"\berror\b|budget exhausted|instruction limit|memory limit", str(line), re.I)
                        for line in sample.get("script_diagnostics", [])), name+": no script error/budget refusal")
        audio = sample.get("audio", {})
        require(audio.get("errors") == 0 and audio.get("dropped") == 0, name+": no mixer error/drop")
    for assertion in expected.get("assertions", []):
        label = assertion.get("label", f"{assertion['capture']}: {assertion['path']} {assertion['op']} {assertion.get('value')!r}")
        try:
            actual = lookup(samples[assertion["capture"]], assertion["path"])
            resolved = dict(assertion)
            if "reference" in assertion:
                reference = assertion["reference"]
                resolved["value"] = lookup(samples[reference["capture"]], reference["path"])
            require(assertion_matches(actual, resolved), label, dict(actual=actual))
        except (KeyError, IndexError, TypeError, ValueError) as error:
            require(False, label, dict(missing_or_invalid=str(error)))
    for item in expected.get("items", []):
        label = f"{item['capture']}: authored item {item['id']} {item['path']} {item['op']}"
        try:
            physics = samples[item["capture"]]["physics"]
            keys = [row[0] for row in physics["authored"] if row[1] == item["id"]]
            bodies = [body for body in physics["bodies"] if body["key"] in keys]
            require(len(keys) == 1 and len(bodies) == 1,
                    label+": exactly one original authored physical body")
            actual = lookup(bodies[0], item["path"])
            require(assertion_matches(actual, item), label, dict(actual=actual))
        except (KeyError, IndexError, TypeError, ValueError) as error:
            require(False, label, dict(missing_or_invalid=str(error)))
    for event in expected.get("events", []):
        actual_count = sum(row["event"] == event["name"] for row in events)
        require(actual_count == event["count"], event.get("label", f"{event['name']}: exactly {event['count']} live events"), actual_count)
    for message in expected.get("log_contains", []):
        require(message in log, "native log contains: "+message)
    for assertion in expected.get("event_assertions", []):
        matching = [event for event in events if event["event"] == assertion["event"]]
        label = assertion.get("label", f"{assertion['event']}: {assertion['path']} {assertion['op']} {assertion.get('value')!r}")
        try:
            state = json.loads(matching[assertion.get("index", -1)]["detail"])
            actual = lookup(state, assertion["path"])
            require(assertion_matches(actual, assertion), label, dict(actual=actual))
        except (KeyError, IndexError, TypeError, ValueError) as error:
            require(False, label, dict(missing_or_invalid=str(error)))
    for agent in expected.get("agents", []):
        names = agent.get("captures", list(samples))
        observations = [observed for name in names for observed in samples.get(name, {}).get("entity_simulation", {}).get("agents", [])
                        if observed.get("id") == agent["id"]]
        require(len(observations) == len(names), agent["id"]+": actual AI observed at each checkpoint")
        positions = [observed["position"] for observed in observations]
        displacement = max((math.dist(left, right) for left in positions for right in positions), default=0)
        require(displacement >= agent.get("min_displacement", .05), agent["id"]+": actual AI movement, not just standing", displacement)
        require(any(observed.get("path_queries", 0) > 0 for observed in observations), agent["id"]+": production planner queried a route")
        if "floor_y" in agent:
            require(bool(positions) and all(agent["floor_y"][0] <= point[1] <= agent["floor_y"][1] for point in positions),
                    agent["id"]+": AI retained the correct storey", positions)
    for event in [event for event in events if event["event"] == "npc_retired_hidden"]:
        state = json.loads(event["detail"])
        attachments = state.get("carry_assemblies", {}).get("attachments", [])
        retired = [attachment for attachment in attachments if attachment.get("retired")]
        require(bool(retired), "retirement receipt identifies the actual carried assembly")
        for attachment in retired:
            actor = attachment["actor"]
            bounds = attachment.get("assembly_bounds")
            require(attachment.get("whole_assembly_hidden") and bounds
                    and all(math.isfinite(value) for value in bounds["min"]+bounds["max"]),
                    actor+": complete posed model and prop bounds occluded at retirement", bounds)
            rendered = [character for character in state.get("character_animations", []) if character.get("id") == actor]
            require(len(rendered) == 1 and rendered[0].get("mesh_visible") is False,
                    actor+": actual rendered character hidden at retirement")
            require(attachment.get("item_mesh_visible") is False,
                    actor+": actual original carried prop hidden at retirement")
    if expected.get("kind") == "hub-visit":
        # Complete commits prove actual package transitions, rather than mere
        # menu selection or an aim action that never reached the doorway.
        log = (folder / "native.log").read_text()
        commits = re.findall(r"\[loading\] committed ([^\s]+)", log)
        require(commits[:3] == [expected["level"], expected["destination"], expected["level"]],
                "actual hub -> correct environment -> hub commits", commits)
        require(len(commits) == 3, "no bounce loop or extra transition", commits)
        for name in ["hub-entry", "destination-entry", "hub-return", "hub-no-bounce"]:
            sample = samples.get(name, {})
            require(sample.get("physics", {}).get("held") is None, name+": hands clear after fresh level load")
            require(sample.get("player_body", {}).get("reset_count") == 0, name+": no void recovery/reset")
    perf = performance(folder, receipt) if receipt.get("performance") else None
    if perf:
        require(perf["measured_frames"] == perf["expected_frames"] and perf["measured_frames"] > 0, "all requested frame timings measured")
        require(perf["all_frames_have_scene_geometry"], "every measured frame submits actual scene geometry")
        require((perf["benchmark_summary"] or {}).get("level") == receipt.get("level"), "measured summary names the requested environment")
        require(bool(perf["physical_drawables"]) and bool(perf["actual_presentation_modes"]), "actual physical drawable and presentation mode recorded")
        require(perf["surface_not_presented_ready"] == 0, "native surface presented throughout timing")
        if require_uncapped:
            require(perf["verified_uncapped"], "throughput exceeds verified refresh without pacing", perf["throughput_status"])
    result = dict(passed=all(check["passed"] for check in checks), checks=checks,
                  performance=perf, visual_review_required=True,
                  note="State receipts do not replace reviewing the native frames, animation/prop occlusion, composition and smooth video.")
    write(folder / "acceptance.json", result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--expectations", type=Path)
    parser.add_argument("--require-uncapped", action="store_true")
    args = parser.parse_args()
    result = audit(args.folder.resolve(strict=True), args.expectations, args.require_uncapped)
    print(json.dumps(dict(passed=result["passed"], checks=len(result["checks"]),
                          failed=[check for check in result["checks"] if not check["passed"]],
                          performance=result["performance"])), flush=True)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
