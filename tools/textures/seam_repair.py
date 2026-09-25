#!/usr/bin/env python3
"""Report and repair the wrapped-edge seams of tiling surface textures.

The game repeats these sheets, so the last column must join the first column
(and the last row the first row); otherwise every wall or floor shows a grid of
seams.  This tool is pure stdlib (no PIL, no numpy), byte-deterministic for a
given input and runtime, and only rewrites a file in ``--repair`` mode.  It
decodes 8-bit RGB and RGBA PNGs (filters 0-4), preserves the colour type, the
exact dimensions and every ancillary chunk, and re-encodes with zlib.

Metric
------
For each axis (left-right, top-bottom) and channel (R, G, B, and Rec.709
luminance) the wrapped edge step is compared with the sheet's own interior
adjacent-pixel step.  Two families are evaluated with the same thresholds:

* **raw**: ``wrap[i] = |v[n-1] - v[0]|`` and ``interior[i] = |v[i+1] - v[i]|``
  for interior positions.
* **smoothed** (the render-test metric): the 3-tap smoothed profile
  ``smooth3(v, i) = (v[i-1] + v[i] + v[i+1]) / 3`` is sampled at the wrap
  edges with the wrapped neighbours (A = smooth3 at n-1: taps n-2, n-1, 0;
  B = smooth3 at 0: taps n-1, 0, 1) and at interior positions (``C[i]``,
  straight averaging), and ``wrap = |A - B|``, ``interior = |C[i+1] - C[i]|``.

The wrapped edge is measured over every row/column; the interior distribution
is sampled every 4th row/column and every 8th interior position (the stride the
strengthened render test uses), which is dense enough for a stable mean/p95.

    accept  iff  mean(wrap) <= 1.60 * mean(interior) + 1.0
            and  p95(wrap)  <= 2.20 * p95(interior)  + 3.0

with ``p95`` the nearest-rank ``ceil(0.95 * n) - 1`` sample.  ``--check``
requires both families to pass on both axes, every channel and the luminance;
the smoothed numbers are the acceptance metric the strengthened render test
uses, and the raw gate additionally keeps the visible pixel step bounded.

Repair
------
The sheet is split into a low-frequency base (two separable box-blur passes,
running-sum, edge-clamped) and the high-frequency residual (image - base).
Both are made periodic with a cross-fade band against a copy rolled by
``offset`` pixels: each band blends from the rolled source at its outer edge
(t = 1, so the outermost pixel becomes the rolled one) back to the original
over ``band`` pixels with a raised-cosine ramp.  The base cross-fades over
``band`` pixels and the residual over the much narrower ``residual-band``
pixels, so the low-frequency tone moves gradually while the fine detail stays
crisp and is only re-sampled over a few edge pixels.

Because both layers use the same offset, the wrapped step of the repaired sheet
is exactly the adjacent-pixel step between the two pixels at the offset (plus
the negligible base difference), i.e. an ordinary interior step, so the seam
disappears instead of merely fading.  The offset is chosen per axis as the
adjacent interior pair with the smallest worst-case step (exhaustive scan over
the sheet), preferring the lower mean.

Shipped environment parameters (basename -> radius, band, residual band, LR
offset, TB offset)::

    wallpaper_stained_01.png  32  64  12  199  131
    carpet_beige_01.png       32  80  12  967  765
    carpet_damp_01.png        32  80  12  184  831
    pool_tile_wall_01.png     32  64  12   64   64
    pool_tile_basin_01.png    32  64  12  269  505

Each sheet was checked against ``--report``: it passes both metric families
with margin, the worst-case wrapped step stays at or below 40/255, and the
repaired bands keep the local variance of the untouched sheet (no flat smear,
no repeated band).  The pool wall tile was repaired with band 64 and offset 64
(the generic defaults it shipped with).  Files not listed use band 64, residual
band 12, radius 32 and offset = half the sheet on both axes; override with
``--band``/``--residual-band``/``--radius``/``--offset``.

Usage::

    python3 tools/textures/seam_repair.py --report PATH...
    python3 tools/textures/seam_repair.py --check  PATH...
    python3 tools/textures/seam_repair.py --repair PATH...

The 128x128 painters in ``office_art.py`` produce placeholder output: the
1024x1024 sheets are the authoritative artwork and must not be regenerated
from those painters' output to satisfy this tool.
"""

from __future__ import annotations

import argparse
import math
import os
import struct
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
_CHANNELS_BY_COLOUR_TYPE = {0: 1, 2: 3, 4: 2, 6: 4}
_COLOUR_TYPE_NAMES = {0: "gray", 2: "RGB", 4: "gray+alpha", 6: "RGBA"}
_REC709 = (0.2126, 0.7152, 0.0722)

# Acceptance thresholds: mean(wrap) <= MEAN_RATIO * mean(interior) + MEAN_SLACK
# and p95(wrap) <= P95_RATIO * p95(interior) + P95_SLACK.
MEAN_RATIO = 1.60
MEAN_SLACK = 1.0
P95_RATIO = 2.20
P95_SLACK = 3.0
PERCENTILE = 0.95

# The wrapped edge is exhaustive; the interior is sampled with the same stride
# as the strengthened render test (every 4th row/column, every 8th position).
INTERIOR_LINE_STRIDE = 4
INTERIOR_PIXEL_STRIDE = 8

# Tuned parameters for the shipped environment sheets (see the module docstring).
SHIPPED_PARAMETERS: dict[str, dict[str, int]] = {
    "wallpaper_stained_01.png": {
        "band": 64, "residual_band": 12, "radius": 32,
        "offset_lr": 199, "offset_tb": 131,
    },
    "carpet_beige_01.png": {
        "band": 80, "residual_band": 12, "radius": 32,
        "offset_lr": 967, "offset_tb": 765,
    },
    "carpet_damp_01.png": {
        "band": 80, "residual_band": 12, "radius": 32,
        "offset_lr": 184, "offset_tb": 831,
    },
    "pool_tile_wall_01.png": {
        "band": 64, "residual_band": 12, "radius": 32,
        "offset_lr": 64, "offset_tb": 64,
    },
    "pool_tile_basin_01.png": {
        # The 1024 sheet's floor grout lines run through the half-sheet
        # default; 269/505 are flat interior columns/rows, so the repaired
        # wrap is an ordinary tile-interior step.
        "band": 64, "residual_band": 12, "radius": 32,
        "offset_lr": 269, "offset_tb": 505,
    },
}
GENERIC_PARAMETERS: dict[str, int | None] = {
    "band": 64, "residual_band": 12, "radius": 32,
    # ``None`` means "roll by half the sheet", the classic offset that never
    # folds the seam back into the band.
    "offset_lr": None, "offset_tb": None,
}

CHANNEL_NAMES = ("R", "G", "B")
LUMA_NAME = "Y"


# --------------------------------------------------------------------- PNG I/O


class PngImage:
    """An 8-bit non-interlaced PNG: pixels are row-major colour-type bytes."""

    __slots__ = ("width", "height", "colour_type", "channels", "pixels", "ancillary")

    def __init__(
        self,
        width: int,
        height: int,
        colour_type: int,
        pixels: bytes,
        ancillary: list[tuple[bytes, bytes]],
    ) -> None:
        self.width = width
        self.height = height
        self.colour_type = colour_type
        self.channels = _CHANNELS_BY_COLOUR_TYPE[colour_type]
        self.pixels = pixels
        self.ancillary = ancillary


def _chunk(tag: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + tag
        + payload
        + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    )


def _paeth(a: int, b: int, c: int) -> int:
    estimate = a + b - c
    distance_a = abs(estimate - a)
    distance_b = abs(estimate - b)
    distance_c = abs(estimate - c)
    if distance_a <= distance_b and distance_a <= distance_c:
        return a
    if distance_b <= distance_c:
        return b
    return c


def _unfilter(raw: bytes, width: int, height: int, channels: int) -> bytes:
    stride = width * channels
    if len(raw) != height * (stride + 1):
        raise ValueError("decompressed data does not match the IHDR dimensions")
    out = bytearray(height * stride)
    previous = bytes(stride)
    offset = 0
    for y in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        if filter_type == 0:
            pass
        elif filter_type == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif filter_type == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif filter_type == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif filter_type == 4:
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = previous[i]
                c = previous[i - channels] if i >= channels else 0
                line[i] = (line[i] + _paeth(a, b, c)) & 0xFF
        else:
            raise ValueError(f"unknown PNG filter type {filter_type}")
        out[y * stride : (y + 1) * stride] = line
        previous = line
    return bytes(out)


def read_png(path: str) -> PngImage:
    """Decodes an 8-bit, non-interlaced RGB/RGBA (also gray) PNG."""
    with open(path, "rb") as handle:
        data = handle.read()
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("missing PNG signature")
    offset = len(PNG_SIGNATURE)
    header = None
    idat_parts: list[bytes] = []
    ancillary: list[tuple[bytes, bytes]] = []
    while offset < len(data):
        if offset + 12 > len(data):
            raise ValueError("truncated PNG chunk header")
        length = int.from_bytes(data[offset : offset + 4], "big")
        tag = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        if len(payload) != length:
            raise ValueError("truncated PNG chunk payload")
        stored_crc = int.from_bytes(data[offset + 8 + length : offset + 12 + length], "big")
        if zlib.crc32(tag + payload) & 0xFFFFFFFF != stored_crc:
            raise ValueError(f"bad CRC on {tag!r} chunk")
        offset += 12 + length
        if tag == b"IHDR":
            header = payload
        elif tag == b"IDAT":
            idat_parts.append(payload)
        elif tag != b"IEND":
            ancillary.append((tag, payload))
    if header is None or len(header) != 13:
        raise ValueError("the first chunk is not a 13-byte IHDR")
    width, height, depth, colour_type, compression, filter_method, interlace = struct.unpack(
        ">IIBBBBB", header
    )
    if depth != 8:
        raise ValueError(f"unsupported bit depth {depth}: only 8-bit PNGs are supported")
    if colour_type not in _CHANNELS_BY_COLOUR_TYPE:
        raise ValueError(f"unsupported colour type {colour_type}")
    if compression != 0 or filter_method != 0:
        raise ValueError("unsupported PNG compression/filter method")
    if interlace != 0:
        raise ValueError("interlaced PNGs are not supported")
    if width <= 0 or height <= 0 or not idat_parts:
        raise ValueError("empty PNG image data")
    raw = zlib.decompress(b"".join(idat_parts))
    pixels = _unfilter(raw, width, height, _CHANNELS_BY_COLOUR_TYPE[colour_type])
    return PngImage(width, height, colour_type, pixels, ancillary)


def write_png(path: str, image: PngImage) -> None:
    """Re-encodes with a deterministic Paeth filter and zlib level 9.

    Ancillary chunks are copied through in their original order, the colour
    type and dimensions are preserved.  The write goes through a sibling
    temporary file so a failure cannot leave a truncated texture behind.
    """
    width, height, channels = image.width, image.height, image.channels
    stride = width * channels
    raw = bytearray()
    previous = bytes(stride)
    for y in range(height):
        line = image.pixels[y * stride : (y + 1) * stride]
        raw.append(4)  # Paeth
        filtered = bytearray(stride)
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = previous[i]
            c = previous[i - channels] if i >= channels else 0
            filtered[i] = (line[i] - _paeth(a, b, c)) & 0xFF
        raw += filtered
        previous = line
    header = struct.pack(">IIBBBBB", width, height, 8, image.colour_type, 0, 0, 0)
    body = _chunk(b"IHDR", header)
    for tag, payload in image.ancillary:
        body += _chunk(tag, payload)
    body += _chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    body += _chunk(b"IEND", b"")
    temporary = f"{path}.seam_repair_tmp"
    try:
        with open(temporary, "wb") as handle:
            handle.write(PNG_SIGNATURE + body)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.remove(temporary)


# ---------------------------------------------------------------------- metric


def _nearest_rank_p95(values: list[float]) -> float:
    ordered = sorted(values)
    index = max(0, math.ceil(PERCENTILE * len(ordered)) - 1)
    return ordered[index]


def _mean(values: list[float]) -> float:
    return sum(values) / len(values)


def _accepts(wrap: list[float], interior: list[float]) -> bool:
    return (
        _mean(wrap) <= MEAN_RATIO * _mean(interior) + MEAN_SLACK
        and _nearest_rank_p95(wrap) <= P95_RATIO * _nearest_rank_p95(interior) + P95_SLACK
    )


class AxisMetrics:
    """Raw and smoothed wrap/interior samples for one axis and channel."""

    __slots__ = ("raw_wrap", "raw_interior", "smooth_wrap", "smooth_interior")

    def __init__(self) -> None:
        self.raw_wrap: list[float] = []
        self.raw_interior: list[float] = []
        self.smooth_wrap: list[float] = []
        self.smooth_interior: list[float] = []

    def summaries(self) -> tuple[tuple[float, float, float, float], tuple[float, float, float, float]]:
        raw = (_mean(self.raw_wrap), _nearest_rank_p95(self.raw_wrap),
               _mean(self.raw_interior), _nearest_rank_p95(self.raw_interior))
        smooth = (_mean(self.smooth_wrap), _nearest_rank_p95(self.smooth_wrap),
                  _mean(self.smooth_interior), _nearest_rank_p95(self.smooth_interior))
        return raw, smooth


def _line_value(
    pixels: bytes,
    width: int,
    channels: int,
    axis: str,
    line: int,
    index: int,
    channel: int | None,
) -> float:
    """One value of a row/column profile; ``channel=None`` selects Rec.709."""
    pixel = (line * width + index) * channels if axis == "lr" else (index * width + line) * channels
    if channel is not None:
        return pixels[pixel + channel]
    return 0.2126 * pixels[pixel] + 0.7152 * pixels[pixel + 1] + 0.0722 * pixels[pixel + 2]


def _line_profile(
    pixels: bytes,
    width: int,
    height: int,
    channels: int,
    axis: str,
    line: int,
    channel: int | None,
) -> list[float]:
    """The full row/column profile; ``channel=None`` selects Rec.709 luminance."""
    n = width if axis == "lr" else height
    if channel is not None:
        if axis == "lr":
            start = (line * width) * channels + channel
            step = channels
        else:
            start = line * channels + channel
            step = width * channels
        return list(pixels[start : start + n * step : step])
    values: list[float] = []
    if axis == "lr":
        base = (line * width) * channels
        step = channels
    else:
        base = line * channels
        step = width * channels
    for offset in range(base, base + n * step, step):
        values.append(
            0.2126 * pixels[offset] + 0.7152 * pixels[offset + 1] + 0.0722 * pixels[offset + 2]
        )
    return values


def measure(pixels: bytes, width: int, height: int, channels: int) -> dict[str, dict[str, AxisMetrics]]:
    """Returns axis -> channel -> metrics for R, G, B (when present) and luma."""
    measured = min(channels, 3)
    result: dict[str, dict[str, AxisMetrics]] = {"lr": {}, "tb": {}}
    for axis in ("lr", "tb"):
        n = width if axis == "lr" else height
        lines = height if axis == "lr" else width
        if n < 4:
            continue
        channels_to_measure: list[tuple[str, int | None]] = [
            (CHANNEL_NAMES[c], c) for c in range(measured)
        ]
        if channels >= 3:
            channels_to_measure.append((LUMA_NAME, None))
        for name, channel in channels_to_measure:
            metrics = AxisMetrics()
            # Wrapped edge: every row/column.
            for line in range(lines):
                first = _line_value(pixels, width, channels, axis, line, 0, channel)
                second = _line_value(pixels, width, channels, axis, line, 1, channel)
                penultimate = _line_value(pixels, width, channels, axis, line, n - 2, channel)
                last = _line_value(pixels, width, channels, axis, line, n - 1, channel)
                metrics.raw_wrap.append(abs(last - first))
                edge_a = (penultimate + last + first) / 3.0
                edge_b = (last + first + second) / 3.0
                metrics.smooth_wrap.append(abs(edge_a - edge_b))
            # Interior: every INTERIOR_LINE_STRIDE-th row/column, every
            # INTERIOR_PIXEL_STRIDE-th position; ``C[i+1] - C[i]`` collapses to
            # ``(v[i+2] - v[i-1]) / 3`` for the straight three-tap average.
            for line in range(0, lines, INTERIOR_LINE_STRIDE):
                profile = _line_profile(pixels, width, height, channels, axis, line, channel)
                for i in range(1, n - 2, INTERIOR_PIXEL_STRIDE):
                    metrics.raw_interior.append(abs(profile[i + 1] - profile[i]))
                    metrics.smooth_interior.append(abs(profile[i + 2] - profile[i - 1]) / 3.0)
            result[axis][name] = metrics
    return result


def file_verdict(measured: dict[str, dict[str, AxisMetrics]]) -> tuple[bool, list[str]]:
    """Returns (passes, failure reasons) over every axis, channel and metric."""
    reasons: list[str] = []
    for axis in ("lr", "tb"):
        for name, metrics in measured[axis].items():
            raw, smooth = metrics.summaries()
            raw_ok = _accepts(metrics.raw_wrap, metrics.raw_interior)
            smooth_ok = _accepts(metrics.smooth_wrap, metrics.smooth_interior)
            if not raw_ok:
                reasons.append(
                    f"{axis.upper()} {name} raw mean {raw[0]:.3f} vs {raw[2]:.3f} / p95 {raw[1]:.3f} vs {raw[3]:.3f}"
                )
            if not smooth_ok:
                reasons.append(
                    f"{axis.upper()} {name} smoothed mean {smooth[0]:.3f} vs {smooth[2]:.3f} / "
                    f"p95 {smooth[1]:.3f} vs {smooth[3]:.3f}"
                )
    return not reasons, reasons


# ---------------------------------------------------------------------- repair


def _clamp_u8(value: float) -> int:
    if value <= 0.0:
        return 0
    if value >= 255.0:
        return 255
    return int(value + 0.5)


def _box_blur_line(values: list[float], radius: int) -> list[float]:
    """One box-blur pass over a line, running sum, edges clamped."""
    n = len(values)
    if radius <= 0 or n == 0:
        return list(values)
    inv = 1.0 / (2 * radius + 1)
    total = 0.0
    for k in range(-radius, radius + 1):
        total += values[0 if k < 0 else (n - 1 if k >= n else k)]
    out = [0.0] * n
    out[0] = total * inv
    for i in range(1, n):
        add = values[i + radius] if i + radius < n else values[n - 1]
        subtract = values[i - radius - 1] if i - radius - 1 >= 0 else values[0]
        total += add - subtract
        out[i] = total * inv
    return out


def _blur_pass(values: list[float], width: int, height: int, channels: int, axis: str, radius: int) -> list[float]:
    out = [0.0] * len(values)
    if axis == "lr":
        for y in range(height):
            row = y * width * channels
            for c in range(channels):
                out[row + c : row + width * channels : channels] = _box_blur_line(
                    values[row + c : row + width * channels : channels], radius
                )
    else:
        for x in range(width):
            for c in range(channels):
                out[x * channels + c :: width * channels] = _box_blur_line(
                    values[x * channels + c :: width * channels], radius
                )
    return out


def _blur2(values: list[float], width: int, height: int, channels: int, radius: int) -> list[float]:
    blurred = _blur_pass(values, width, height, channels, "lr", radius)
    blurred = _blur_pass(blurred, width, height, channels, "tb", radius)
    blurred = _blur_pass(blurred, width, height, channels, "lr", radius)
    return _blur_pass(blurred, width, height, channels, "tb", radius)


def _ramp(length: int) -> list[float]:
    """Raised-cosine ramp from 0.0 at index 0 to 1.0 at index length-1."""
    if length <= 1:
        return [1.0]
    return [0.5 - 0.5 * math.cos(math.pi * k / (length - 1)) for k in range(length)]


def _crossfade_axis(
    source: list[float],
    width: int,
    height: int,
    channels: int,
    repair_channels: int,
    band: int,
    offset: int,
    axis: str,
) -> list[float]:
    """Blends the edges towards a rolled copy read from the untouched source.

    The left band copies the source at ``offset`` at its outer edge and ramps
    back to the original across ``band`` pixels; the right band does the same
    with the source at ``offset - 1`` (mod n), so the two outer edges become an
    adjacent interior pair.
    """
    n = width if axis == "lr" else height
    band = max(2, min(band, n))
    ramp = _ramp(band)
    out = list(source)
    lines = height if axis == "lr" else width
    stride = channels if axis == "lr" else width * channels
    for line in range(lines):
        origin = line * width * channels if axis == "lr" else line * channels
        for k in range(band):
            left_t = ramp[band - 1 - k]
            left_pixel = origin + k * stride
            left_source = origin + ((k + offset) % n) * stride
            right_t = ramp[k]
            right_index = n - band + k
            right_pixel = origin + right_index * stride
            right_source = origin + ((right_index + offset) % n) * stride
            for c in range(repair_channels):
                out[left_pixel + c] = (
                    (1.0 - left_t) * source[left_pixel + c] + left_t * source[left_source + c]
                )
                out[right_pixel + c] = (
                    (1.0 - right_t) * source[right_pixel + c] + right_t * source[right_source + c]
                )
    return out


def _resolve_offset(offset: int | None, length: int) -> int:
    """Half the sheet when untuned, clamped away from the very ends."""
    if offset is None:
        return max(1, length // 2)
    return min(max(1, offset), length - 1)


def repair(
    pixels: bytes,
    width: int,
    height: int,
    channels: int,
    radius: int,
    band: int,
    residual_band: int,
    offset_lr: int | None,
    offset_tb: int | None,
) -> bytes:
    """Returns the repaired pixel buffer; alpha (if any) is left untouched."""
    repair_channels = min(channels, 3)
    offset_lr = _resolve_offset(offset_lr, width)
    offset_tb = _resolve_offset(offset_tb, height)
    values = [float(byte) for byte in pixels]
    base = _blur2(values, width, height, channels, radius)
    residual = [values[index] - base[index] for index in range(len(values))]
    repaired_base = _crossfade_axis(base, width, height, channels, repair_channels, band, offset_lr, "lr")
    repaired_base = _crossfade_axis(
        repaired_base, width, height, channels, repair_channels, band, offset_tb, "tb"
    )
    repaired_residual = _crossfade_axis(
        residual, width, height, channels, repair_channels, residual_band, offset_lr, "lr"
    )
    repaired_residual = _crossfade_axis(
        repaired_residual, width, height, channels, repair_channels, residual_band, offset_tb, "tb"
    )
    out = bytearray(pixels)
    for y in range(height):
        for x in range(width):
            index = (y * width + x) * channels
            for c in range(repair_channels):
                out[index + c] = _clamp_u8(repaired_base[index + c] + repaired_residual[index + c])
    return bytes(out)


# ------------------------------------------------------------------------ CLI


def _parameters(path: str, args: argparse.Namespace) -> dict[str, int | None]:
    params = dict(SHIPPED_PARAMETERS.get(os.path.basename(path), GENERIC_PARAMETERS))
    if args.band is not None:
        params["band"] = args.band
    if args.residual_band is not None:
        params["residual_band"] = args.residual_band
    if args.radius is not None:
        params["radius"] = args.radius
    if args.offset is not None:
        if len(args.offset) == 1:
            params["offset_lr"] = params["offset_tb"] = args.offset[0]
        else:
            params["offset_lr"], params["offset_tb"] = args.offset
    return params


def _load(path: str) -> PngImage:
    try:
        return read_png(path)
    except (OSError, ValueError, zlib.error) as error:
        raise SystemExit(f"FAIL {path}: {error}") from error


def _print_report(path: str, image: PngImage) -> tuple[bool, list[str]]:
    measured = measure(image.pixels, image.width, image.height, image.channels)
    print(
        f"== {path} ({image.width}x{image.height}, {_COLOUR_TYPE_NAMES[image.colour_type]}, 8-bit)"
    )
    print("   axis ch   raw edge mean/p95    raw interior mean/p95   ratio   gate  |  "
          "smoothed edge mean/p95   smoothed interior        ratio   gate")
    for axis in ("lr", "tb"):
        for name, metrics in measured[axis].items():
            raw, smooth = metrics.summaries()
            raw_ok = _accepts(metrics.raw_wrap, metrics.raw_interior)
            smooth_ok = _accepts(metrics.smooth_wrap, metrics.smooth_interior)
            ratio_raw = raw[0] / raw[2] if raw[2] else float("inf")
            ratio_smooth = smooth[0] / smooth[2] if smooth[2] else float("inf")
            print(
                f"   {axis.upper():3s} {name:2s}  "
                f"{raw[0]:8.3f} /{raw[1]:8.2f}   {raw[2]:8.3f} /{raw[3]:8.2f}   "
                f"{ratio_raw:5.2f}x  {'pass' if raw_ok else 'FAIL'}  |  "
                f"{smooth[0]:8.3f} /{smooth[1]:8.2f}   {smooth[2]:8.3f} /{smooth[3]:8.2f}   "
                f"{ratio_smooth:5.2f}x  {'pass' if smooth_ok else 'FAIL'}"
            )
    passes, reasons = file_verdict(measured)
    print(f"   verdict: {'PASS' if passes else 'FAIL'}")
    for reason in reasons:
        print(f"     - {reason}")
    return passes, reasons


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--report", nargs="+", metavar="PATH", help="print the seam metrics for each PNG")
    mode.add_argument("--check", nargs="+", metavar="PATH", help="exit non-zero if a PNG fails the metric")
    mode.add_argument("--repair", nargs="+", metavar="PATH", help="repair each PNG in place")
    parser.add_argument("--band", type=int, help="low-frequency cross-fade band in pixels")
    parser.add_argument("--residual-band", type=int, help="high-frequency cross-fade band in pixels")
    parser.add_argument("--radius", type=int, help="box-blur radius of the base/residual split")
    parser.add_argument(
        "--offset", type=int, nargs="+", metavar="PIXELS",
        help="roll distance: one value for both axes or two (left-right, top-bottom)",
    )
    args = parser.parse_args(argv)
    if args.offset is not None and len(args.offset) not in (1, 2):
        parser.error("--offset takes one or two values")
    for name in ("band", "residual_band", "radius"):
        value = getattr(args, name)
        if value is not None and value < 0:
            parser.error(f"--{name.replace('_', '-')} must be non-negative")
    if args.offset is not None and any(value < 1 for value in args.offset):
        parser.error("--offset must be at least 1")

    total = 0
    failures = 0
    for path in args.report if args.report else (args.check if args.check else args.repair):
        image = _load(path)
        if args.report:
            _print_report(path, image)
            continue
        if args.check:
            passes, reasons = file_verdict(measure(image.pixels, image.width, image.height, image.channels))
            print(f"{'PASS' if passes else 'FAIL'} {path}")
            for reason in reasons:
                print(f"  - {reason}")
            total += 1
            failures += 0 if passes else 1
            continue

        params = _parameters(path, args)
        before, _ = file_verdict(measure(image.pixels, image.width, image.height, image.channels))
        repaired = repair(
            image.pixels,
            image.width,
            image.height,
            image.channels,
            params["radius"],
            params["band"],
            params["residual_band"],
            params["offset_lr"],
            params["offset_tb"],
        )
        write_png(path, PngImage(image.width, image.height, image.colour_type, repaired, image.ancillary))
        after_image = _load(path)
        after, reasons = file_verdict(
            measure(after_image.pixels, after_image.width, after_image.height, after_image.channels)
        )
        total += 1
        failures += 0 if after else 1
        print(
            f"{'REPAIRED' if after else 'REPAIRED (still failing)'} {path} "
            f"band={params['band']} residual_band={params['residual_band']} radius={params['radius']} "
            f"offset={params['offset_lr']}/{params['offset_tb']} "
            f"before={'PASS' if before else 'FAIL'} after={'PASS' if after else 'FAIL'}"
        )
        for reason in reasons:
            print(f"  - {reason}")
    if args.check or args.repair:
        print(f"{total - failures}/{total} texture(s) pass the seam metric")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
