#!/usr/bin/env python3
"""Project an offline lighting dump onto a world plane for stage comparison.

Requires the optional NumPy and Pillow analysis tools; neither is a game
dependency. Values are linear receiver RGB, with one fixed display maximum.
The chart and density modes expose segmentation without modifying textures.
Indirect reads the saved component; subtracting reconstructed combined values
would not preserve the nonlinear directional reconstruction's meaning.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw


def audit_records(records: list[dict]) -> dict:
    """Validate chart geometry and disjoint padded atlas reservations."""
    occupied = np.zeros((8,1024,1024),dtype=bool)
    errors = []
    one_sample_axes = elongated = texels = 0
    pages = set()
    for record in records:
        index = record["index"]
        x,y,width,height = record["rectangle"]
        page = record["page"]
        gutter = 1 if record["kind"] == "prop" else 2
        if (any(not isinstance(v,int) for v in (x,y,width,height,page))
                or not 0<=page<8 or width<=0 or height<=0
                or x<gutter or y<gutter or x+width+gutter>1024 or y+height+gutter>1024):
            errors.append(dict(chart=index,reason="Invalid padded atlas bounds"))
            continue
        vectors = np.array([record[key] for key in
            ("origin","u_axis","v_axis","diagonal_correction","geometric_normal")])
        densities = np.array(record["texels_per_metre"])
        if vectors.shape != (5,3) or densities.shape != (2,) or not np.isfinite(vectors).all() or not np.isfinite(densities).all() or np.any(densities<0):
            errors.append(dict(chart=index,reason="Nonfinite or invalid chart geometry/density"))
            continue
        _,u,v,correction,normal = vectors
        area = np.linalg.norm(np.cross(u,v)) if record["triangle"] else (
            np.linalg.norm(np.cross(u,u+v+correction))+np.linalg.norm(np.cross(u+v+correction,v)))
        lengths = np.linalg.norm([u,v],axis=1)
        if area<=1e-12 or np.any(lengths<=1e-10) or abs(np.linalg.norm(normal)-1)>1e-4:
            errors.append(dict(chart=index,reason="Degenerate chart or nonunit geometric normal"))
            continue
        reservation = occupied[page,y-gutter:y+height+gutter,x-gutter:x+width+gutter]
        if reservation.any():
            errors.append(dict(chart=index,reason="Overlapping padded atlas reservations"))
        reservation[:] = True
        one_sample_axes += int(width==1)+int(height==1)
        elongated += int(lengths.max()/lengths.min()>100)
        texels += width*height
        pages.add(page)
    return dict(charts=len(records),pages=len(pages),receiver_texels=texels,
        one_sample_axes=one_sample_axes,world_axis_ratio_over_100=elongated,
        error_count=len(errors),errors=errors[:256],
        interpretation="One-sample axes and elongated charts are budget/pathology diagnostics, not automatic errors")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dump", type=Path)
    parser.add_argument("--room", type=int, help="Omit to inspect any room or model")
    parser.add_argument("--kind", default="floor")
    parser.add_argument("--axes", default="x,z")
    parser.add_argument("--normal", help="Select an oriented face, e.g. 0,0,-1")
    parser.add_argument("--plane", type=float, help="Coordinate on the unprojected axis")
    parser.add_argument("--stage", default="filled", help="Stage name, indirect, chart-ids, or density")
    parser.add_argument("--bounds", help="Projected bounds: minimum A,B,maximum A,B")
    parser.add_argument("--legacy-uv", action="store_true", help="Reconstruct the previous runtime atlas coordinates")
    parser.add_argument("--audit", action="store_true", help="Write geometry/padded-reservation validation JSON instead of a projection")
    parser.add_argument("--maximum", type=float, default=0.5)
    parser.add_argument("--edge", type=int, default=768)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.maximum <= 0 or not np.isfinite(args.maximum) or not 32 <= args.edge <= 4096:
        parser.error("Positive finite display maximum and edge 32..4096 required")
    try:
        axes = ["xyz".index(axis) for axis in args.axes.split(",")]
    except ValueError:
        parser.error("Axes must be two of x,y,z")
    if len(axes) != 2 or axes[0] == axes[1]:
        parser.error("Axes must be distinct")
    normal = None
    if args.normal:
        try:
            normal = np.array([float(v) for v in args.normal.split(",")])
        except ValueError:
            parser.error("Normal must contain three finite numbers")
        if normal.shape != (3,) or not np.isfinite(normal).all() or np.linalg.norm(normal) < 1e-10:
            parser.error("Normal must be finite and nonzero")
        normal /= np.linalg.norm(normal)
    if args.plane is not None and not np.isfinite(args.plane):
        parser.error("Plane coordinate must be finite")
    bounds_override = None
    if args.bounds:
        try:
            bounds_override = np.array([float(v) for v in args.bounds.split(",")]).reshape(2,2)
        except ValueError:
            parser.error("Bounds must contain four finite coordinates")
        if not np.isfinite(bounds_override).all() or np.any(bounds_override[1] <= bounds_override[0]):
            parser.error("Bounds must have finite positive spans")
    unprojected = next(axis for axis in range(3) if axis not in axes)
    records = json.loads((args.dump / "charts.json").read_text())
    if args.audit:
        result = audit_records(records)
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps(result,indent=2)+"\n")
        print(json.dumps(result))
        return int(result["error_count"]>0)
    offset = 0
    selected = []
    for record in records:
        record["offset"] = offset
        offset += record["rectangle"][2] * record["rectangle"][3]
        if ((args.room is None or record["room"] == args.room) and record["kind"] == args.kind
                and (normal is None or np.dot(normal,record["geometric_normal"]) > 0.99999)
                and (args.plane is None or abs(record["origin"][unprojected]-args.plane) < 1e-4)):
            selected.append(record)
    if not selected:
        parser.error("No matching charts")
    values = None
    if args.stage not in ("chart-ids", "density"):
        path = args.dump / (args.stage + ".rgb-f32le")
        if path.stat().st_size != offset * 12:
            parser.error("Stage length does not match chart metadata")
        values = np.memmap(path, dtype="<f4", mode="r", shape=(offset, 3))
    corners = [np.array(r["origin"]) + u*np.array(r["u_axis"]) + v*np.array(r["v_axis"])
               + min(u,v)*np.array(r["diagonal_correction"])
               for r in selected for u,v in ((0,0),(1,0),(0,1),(1,1)) if not r["triangle"] or u+v <= 1]
    bounds = np.array(corners)[:, axes]
    low, high = bounds.min(axis=0), bounds.max(axis=0)
    if bounds_override is not None:
        low, high = bounds_override
    span = high-low
    if np.any(span <= 0):
        parser.error("Projection is degenerate")
    width, height = np.maximum(1, np.rint(span/span.max()*args.edge).astype(int))
    xs, ys = np.meshgrid(np.linspace(low[0],high[0],width), np.linspace(high[1],low[1],height))
    pixels = np.full((height,width,3), 24, dtype=np.uint8)
    metrics = []
    for r in selected:
        origin = np.array(r["origin"])[axes]
        correction = np.array(r["diagonal_correction"])[axes]
        u_axis, v_axis = np.array([r["u_axis"],r["v_axis"]])[:,axes]
        transform = np.array([u_axis,v_axis if r["triangle"] else v_axis+correction]).T
        if abs(np.linalg.det(transform)) < 1e-10:
            continue
        uv = np.linalg.solve(transform, np.stack((xs-origin[0],ys-origin[1])).reshape(2,-1)).reshape(2,height,width)
        if not r["triangle"] and np.any(correction):
            transform = np.array([u_axis+correction,v_axis]).T
            if abs(np.linalg.det(transform)) < 1e-10:
                continue
            other = np.linalg.solve(transform, np.stack((xs-origin[0],ys-origin[1])).reshape(2,-1)).reshape(2,height,width)
            uv = np.where((uv[0] >= uv[1])[None,:,:],uv,other)
        mask = ((uv >= 0)&(uv <= 1)).all(axis=0)
        if r["triangle"]:
            mask &= uv.sum(axis=0) <= 1
        if not mask.any():
            continue
        chart_width, chart_height = r["rectangle"][2:]
        if args.stage == "chart-ids":
            rng = np.random.default_rng(r["index"])
            rgb = rng.integers(48,240,size=3,dtype=np.uint8)
            pixels[mask] = rgb
        elif args.stage == "density":
            density = min(r["texels_per_metre"])
            pixels[mask] = round(min(1,density/args.maximum)*255)
        else:
            chart = values[r["offset"]:r["offset"]+chart_width*chart_height].reshape(chart_height,chart_width,3)
            x = np.clip(uv[0][mask]*chart_width-0.5,0,chart_width-1) if args.legacy_uv else np.clip(uv[0][mask],0,1)*(chart_width-1)
            y = np.clip(uv[1][mask]*chart_height-0.5,0,chart_height-1) if args.legacy_uv else np.clip(uv[1][mask],0,1)*(chart_height-1)
            ix, iy = x.astype(int), y.astype(int)
            fx, fy = (x-ix)[:,None], (y-iy)[:,None]
            jx, jy = np.minimum(ix+1,chart_width-1),np.minimum(iy+1,chart_height-1)
            rgb = chart[iy,ix]*(1-fx)*(1-fy)+chart[iy,jx]*fx*(1-fy)+chart[jy,ix]*(1-fx)*fy+chart[jy,jx]*fx*fy
            pixels[mask] = np.rint(np.clip(rgb/args.maximum,0,1)**(1/2.2)*255).astype(np.uint8)
            metrics.append(dict(chart=r["index"], minimum=float(chart.min()),maximum=float(chart.max()),mean=float(chart.mean())))
    canvas = Image.new("RGB",(width,height+48),(24,24,24))
    canvas.paste(Image.fromarray(pixels),(0,48))
    ImageDraw.Draw(canvas).text((8,8),f"{args.dump.name} / {args.stage} / room {args.room} {args.kind}\n{args.axes} bounds {low.round(3)} .. {high.round(3)}; white = {args.maximum}; legacy UV = {args.legacy_uv}",fill=(255,255,255))
    args.out.parent.mkdir(parents=True,exist_ok=True)
    canvas.save(args.out)
    print(json.dumps(dict(image=str(args.out),charts=len(selected),metrics=metrics)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
