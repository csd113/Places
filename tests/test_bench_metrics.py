"""Known-answer benchmark accounting; no binary, GPU, assets, or wall-clock assertions."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools.bench.loading import (
    install_packages,
    package_identity,
    parse_window_size,
    read_trace,
    trace_metrics,
)
from tools.bench.lightmap_report import parse_logs
from tools.bench.capture_snowfall import expected_budgets, validate_run


class WinterNativeEvidenceTests(unittest.TestCase):
    def test_rejects_occlusion_missing_world_frames_and_invalid_particle_evidence(self):
        events = [dict(event='present', detail='ready')]
        frames = [dict(total_vertices='100', visible_vertices='60', draw_calls='4',
                       frame_ms='8', render_ms='7')]
        snow = [dict(seconds='1', evaluated='700', submitted='100', sheltered='50',
                     culled='550', capacity_growth='0', quad_screen_coverage='.01', sync_us='80')]
        self.assertEqual(validate_run(events, frames, snow, {700})['ready_not_presented'], 0)
        for bad_events, bad_frames, bad_snow in (
            ([], frames, snow),
            (events + [dict(event='surface_not_presented', detail='ready')], frames, snow),
            (events, [], snow),
            (events, [{**frames[0], 'draw_calls':'0'}], snow),
            (events, [{**frames[0], 'frame_ms':'nan'}], snow),
            (events, frames, []),
            (events, frames, [{**snow[0], 'capacity_growth':'1'}]),
            (events, frames, [{**snow[0], 'evaluated':'2049'}]),
            (events, frames, [{**snow[0], 'culled':'549'}]),
            (events, frames, snow + [{**snow[0], 'seconds':'.5'}]),
            (events, frames, [{**snow[0], 'quad_screen_coverage':'inf'}]),
        ):
            with self.assertRaises(RuntimeError):
                validate_run(bad_events, bad_frames, bad_snow, {700})

    def test_completed_frame_campaign_and_live_quality_cycle_must_be_complete(self):
        self.assertEqual(expected_budgets({'kind':'snow'}, ('high', 'medium', 'low')),
                         {1400, 1050, 700})
        self.assertEqual(expected_budgets({'count':1400, 'intensity':.7}, ('high',)), {980})
        events = [dict(event='present', detail='ready')] * 720
        frames = [dict(total_vertices='100', visible_vertices='60', draw_calls='4',
                       frame_ms='8', render_ms='7')] * 600
        self.assertEqual(validate_run(events, frames, [], {1400}, performance=True)
                         ['ready_presentations'], 720)
        with self.assertRaises(RuntimeError):
            validate_run(events[:-1], frames, [], {1400}, performance=True)
        with self.assertRaises(RuntimeError):
            validate_run(events, frames[:-1], [], {1400}, performance=True)
        snow = [dict(seconds=str(i), evaluated=str(n), submitted='0', sheltered='0',
                     culled=str(n), capacity_growth='0', quad_screen_coverage='0', sync_us='1')
                for i, n in enumerate((1400, 700, 1050, 1400))]
        validate_run(events, frames, snow, {1400, 700, 1050}, cycle=True)
        with self.assertRaises(RuntimeError):
            validate_run(events, frames, snow[:2], {1400, 700, 1050}, cycle=True)


def event(kind, at, request=1, detail=""):
    return {"event": kind, "elapsed_ms": at, "request": request, "detail": detail}


class LoadingMetricsTests(unittest.TestCase):
    def test_window_size_option_parses_and_rejects_bad_values(self):
        import argparse

        self.assertEqual(parse_window_size("640x360"), (640, 360))
        self.assertEqual(parse_window_size("1512x850"), (1512, 850))
        for bad in ["640", "640X360", "0x360", "640x0", "-640x360", "640x360x2", "", " 640x 360"]:
            with self.assertRaises(argparse.ArgumentTypeError, msg=bad):
                parse_window_size(bad)

    def test_install_package_copies_into_the_run_state_and_records_identity(self):
        import hashlib
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / "private_map.placesmap"
            package.write_bytes(b"PLPM-test-bytes")
            destination = root / "state" / "levels"
            destination.mkdir(parents=True)
            identities = install_packages([package], destination)
            self.assertEqual(identities, [{
                "path": str(package),
                "sha256": hashlib.sha256(b"PLPM-test-bytes").hexdigest(),
            }])
            self.assertEqual((destination / "private_map.placesmap").read_bytes(),
                             b"PLPM-test-bytes")
            with self.assertRaises(ValueError):
                install_packages([root / "missing.placesmap"], destination)
            with self.assertRaises(ValueError):
                package_identity(root / "authoring_source.json")
            self.assertEqual(package_identity(package)["sha256"],
                             hashlib.sha256(b"PLPM-test-bytes").hexdigest())

    def test_missing_trace_is_unavailable_not_zero(self):
        result = trace_metrics([])
        self.assertFalse(result["trace_available"])
        self.assertIsNone(result["first_present_ms"])
        self.assertIsNone(result["first_ready_present_ms"])
        for field in ["event_pump_gaps", "present_gaps", "loading_event_pump_gaps", "loading_present_gaps"]:
            self.assertEqual(result[field]["count"], 0)
            self.assertIsNone(result[field]["max_ms"])
        self.assertIsNone(result["event_pump_gaps"]["initial_wait_ms"])

    def test_initial_freeze_and_loading_intervals_are_separate_from_ready_play(self):
        events = [event("entry", 0), event("request", 10), event("event_pump", 100),
                  event("present", 105, detail="loading"), event("event_pump", 110),
                  event("present", 115, detail="loading"), event("event_pump", 120),
                  event("scene_presented", 120, detail="small"), event("present", 120, detail="ready"),
                  event("event_pump", 1120), event("present", 1125, detail="ready"),
                  event("shutdown_requested", 1130), event("event_pump", 9000),
                  event("shutdown_complete", 9100)]
        result = trace_metrics(events)
        self.assertEqual(result["loading_windows_ms"], [(10, 120)])
        self.assertEqual(result["event_pump_gaps"]["initial_wait_ms"], 100)
        self.assertEqual(result["event_pump_gaps"]["max_ms"], 1000)
        self.assertEqual(result["loading_event_pump_gaps"]["max_ms"], 100)
        self.assertEqual(result["loading_present_gaps"]["max_ms"], 105)
        self.assertEqual(result["first_present_ms"], 105)
        self.assertEqual(result["first_ready_present_ms"], 120)
        self.assertNotEqual(result["event_pump_gaps"]["max_ms"], 7880,
                            "shutdown draining is reported separately from running/loading")

    def test_supersede_cancel_failure_and_graphics_ready_bound_windows(self):
        events = [event("entry", 0), event("request", 10, 1), event("request", 20, 2),
                  event("cancel", 30, 2), event("request", 40, 3), event("failed", 50, 3),
                  event("request", 60, 4), event("present", 75, 4, "ready"),
                  event("shutdown_requested", 100)]
        self.assertEqual(trace_metrics(events)["loading_windows_ms"],
                         [(10, 20), (20, 30), (40, 50), (60, 75)])

    def test_percentiles_have_known_nearest_rank_and_enqueue_latencies(self):
        events = [event("entry", 0), event("event_pump", 10), event("event_pump", 30),
                  event("event_pump", 60), event("event_pump", 100),
                  event("action", 110, detail=json.dumps({"latency_ms": 70})),
                  event("action", 120, detail=json.dumps({"latency_ms": 5}))]
        result = trace_metrics(events)
        self.assertEqual(result["event_pump_gaps"]["count"], 4)
        self.assertEqual(result["event_pump_gaps"]["p50_ms"], 20)
        self.assertEqual(result["event_pump_gaps"]["p95_ms"], 40)
        self.assertEqual(result["input_latency"]["max_ms"], 70)
        self.assertEqual(result["input_latency"]["p50_ms"], 5)

    def test_interrupted_json_trace_keeps_prior_evidence_and_reports_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.jsonl"
            path.write_text(json.dumps(event("entry", 0)) + '\n{"event":')
            events, error = read_trace(path)
            self.assertEqual(events, [event("entry", 0)])
            self.assertIn("trace line 2", error)
            absent, no_error = read_trace(Path(directory) / "absent")
            self.assertEqual(absent, [])
            self.assertIsNone(no_error)

    def test_settings_transitions_are_separate_from_loading_and_steady_state(self):
        # One immediate apply and one rebuild that requires a world commit; the
        # named settings_transition metric covers exactly those windows.
        events = [
            event("entry", 0),
            event("window_ready", 5),
            event("request", 10),
            event("scene_presented", 60, detail="small"),
            event("ready", 60),
            event("settings_change", 100, detail=json.dumps({"quality": "low"})),
            event("settings_applied", 104),
            event("settings_change", 200, detail=json.dumps({"lightmaps": "full"})),
            event("request", 205),
            event("gpu_ready", 260),
            event("ready", 260),
            event("present", 262, detail="ready"),
            event("shutdown_requested", 300),
        ]
        result = trace_metrics(events)
        self.assertEqual(result["settings_transition_windows_ms"], [(100, 104), (200, 260)])
        self.assertEqual(result["settings_transition"]["count"], 2)
        self.assertEqual(result["settings_transition"]["p50_ms"], 4)
        self.assertEqual(result["settings_transition"]["max_ms"], 60)
        self.assertEqual(result["loading_windows_ms"], [(10, 60), (205, 262)])
        self.assertEqual(result["package_loading"]["count"], 2)
        self.assertEqual(result["binary_startup_ms"], 5)
        self.assertEqual(result["first_usable_scene_ms"], 60)
        self.assertEqual(result["first_ready_present_ms"], result["first_usable_scene_ms"])

    def test_unfinished_settings_transition_reports_the_open_interval(self):
        # A rebuild that never commits still reports from the change to the
        # shutdown boundary rather than disappearing.
        events = [
            event("entry", 0),
            event("window_ready", 2),
            event("settings_change", 50, detail="{}"),
            event("shutdown_requested", 90),
        ]
        result = trace_metrics(events)
        self.assertEqual(result["settings_transition_windows_ms"], [(50, 90)])
        self.assertEqual(result["settings_transition"]["max_ms"], 40)

    def test_metrics_are_unavailable_not_zero_without_a_trace(self):
        result = trace_metrics([])
        self.assertIsNone(result["binary_startup_ms"])
        self.assertIsNone(result["first_usable_scene_ms"])
        self.assertEqual(result["package_loading"]["count"], 0)
        self.assertEqual(result["settings_transition"]["count"], 0)
        self.assertIsNone(result["settings_transition"]["max_ms"])


class LightmapSegmentTests(unittest.TestCase):
    def test_last_commit_counts_and_timings_never_include_other_worlds(self):
        text = """[loading] compiled-cache miss level=demo
[loading] committed demo
[level] 100 static vertices, 200 prop vertices, built in 900.0 ms (lighting 500.0 + props 300.0 + surfaces 100.0)
[loading] compiled-cache hit level=small
[loading] committed small
[level] 4 static vertices, 6 prop vertices, built in 9.0 ms (lighting 5.0 + props 3.0 + surfaces 1.0)
[lightmaps] 1 page(s), 2 chart(s), 3 chart texels, 4 page texels (5 KiB), filled in 6.0 ms
[loading] compiled-cache miss level=cancelled
"""
        result = parse_logs(text)
        self.assertEqual(result["level_id"], "small")
        self.assertEqual(result["prepared_cache"], "hit")
        self.assertEqual(result["static_vertices"], 4)
        self.assertEqual(result["prop_vertices"], 6)
        self.assertEqual(result["build_ms"], 9)
        self.assertEqual(result["lighting_ms"], 5)
        self.assertEqual(result["lightmap_bake_ms"], 6)

    def test_uncommitted_same_level_request_cannot_relabel_previous_hit(self):
        text = """[loading] compiled-cache hit level=small
[loading] committed small
[level] 4 static vertices, 6 prop vertices, built in 9.0 ms (lighting 5.0 + props 3.0 + surfaces 1.0)
[loading] compiled-cache miss level=small
"""
        self.assertEqual(parse_logs(text)["prepared_cache"], "hit")


if __name__ == "__main__":
    unittest.main()
