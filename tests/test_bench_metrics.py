"""Known-answer benchmark accounting; no binary, GPU, assets, or wall-clock assertions."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools.bench.loading import read_trace, trace_metrics
from tools.bench.lightmap_report import parse_logs


def event(kind, at, request=1, detail=""):
    return {"event": kind, "elapsed_ms": at, "request": request, "detail": detail}


class LoadingMetricsTests(unittest.TestCase):
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
