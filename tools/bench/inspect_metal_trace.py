#!/usr/bin/env python3
"""Summarize exported Metal System Trace GPU and allocation tables.

Export metal-gpu-intervals and metal-current-allocated-size with xctrace.
GPU work is the union of active target intervals, divided by the number of
scene encoders. It measures device occupancy per rendered frame, not CPU
submission or a sum that double-counts overlapping Vertex/Fragment work.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import statistics
import xml.etree.ElementTree as ET


def rows(path: Path):
    node = ET.parse(path).getroot().find("node")
    if node is None:
        raise ValueError(f"Missing table in {path}")
    references = {element.attrib["id"]: element for element in node.iter() if "id" in element.attrib}
    schema = node.find("schema")
    columns = [column.find("mnemonic").text for column in schema.findall("col")]
    for row in node.findall("row"):
        values = []
        for element in row:
            if "ref" in element.attrib:
                element = references[element.attrib["ref"]]
            values.append(element.text or element.get("fmt", ""))
        yield dict(zip(columns, values))


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[round((len(ordered)-1)*fraction)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gpu", type=Path, required=True)
    parser.add_argument("--memory", type=Path, required=True)
    parser.add_argument("--process", default="places (")
    parser.add_argument("--start", type=float, default=1.0, help="Stable trace window, seconds")
    parser.add_argument("--end", type=float, default=7.0)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not 0 <= args.start < args.end:
        parser.error("A nonnegative ordered trace window is required")
    start, end = round(args.start*1e9), round(args.end*1e9)
    intervals = []
    scenes = {}
    processes = set()
    for row in rows(args.gpu):
        if not row["process"].startswith(args.process) or row["state"] != "Active":
            continue
        first = int(row["start"])
        last = first+int(row["duration"])
        if first >= end or last <= start:
            continue
        processes.add(row["process"])
        intervals.append((max(start,first),min(end,last)))
        if "places-wgpu-scene:" in row["event-label"] and start <= first < end:
            encoder = row["encoder-id"]
            bounds = scenes.setdefault(encoder,[first,last])
            bounds[0] = min(bounds[0],first)
            bounds[1] = max(bounds[1],last)
    if len(processes) != 1 or not scenes:
        parser.error("Expected one traced Places process with labelled scene encoders")
    occupied = 0
    first = last = start
    for a,b in sorted(intervals):
        if a > last:
            occupied += last-first
            first,last = a,b
        else:
            last = max(last,b)
    occupied += last-first
    spans = [(b-a)/1e6 for a,b in scenes.values() if b<=end]
    allocation_events = sorted((int(row["start"]),int(row["current-allocated-size"]))
              for row in rows(args.memory) if row["process"].startswith(args.process))
    memory = [size for time,size in allocation_events if start<=time<end]
    if not memory:
        parser.error("No allocation samples in the requested stable window")
    # Allocation rows are state changes, not evenly spaced samples. Weight the
    # resident allocation by its duration so frequent short-lived allocations
    # cannot bias the reported median toward one stage of each frame.
    durations = {}
    for index,(time,size) in enumerate(allocation_events):
        next_time = allocation_events[index+1][0] if index+1<len(allocation_events) else end
        duration = max(0,min(end,next_time)-max(start,time))
        if duration:
            durations[size] = durations.get(size,0)+duration
    covered = sum(durations.values())
    if covered != end-start:
        parser.error("Allocation state does not cover the entire requested window")
    cumulative = 0
    for size,duration in sorted(durations.items()):
        cumulative += duration
        if cumulative*2 >= covered:
            time_median = size
            break
    result = dict(process=next(iter(processes)),window_seconds=args.end-args.start,
        scene_encoders=len(scenes),gpu_occupied_ms=occupied/1e6,
        gpu_occupied_mean_per_frame_ms=occupied/1e6/len(scenes),
        scene_gpu_span_mean_ms=statistics.mean(spans),scene_gpu_span_median_ms=statistics.median(spans),
        scene_gpu_span_p95_ms=percentile(spans,.95),allocation_samples=len(memory),
        metal_allocated_median_bytes=statistics.median(memory),metal_allocated_peak_bytes=max(memory),
        metal_allocated_time_median_bytes=time_median,
        metal_allocated_time_mean_bytes=sum(size*duration for size,duration in durations.items())/covered,
        method="Union of active target GPU intervals / scene encoders; overlapping hardware channels counted once")
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
