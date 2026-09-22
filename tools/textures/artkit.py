#!/usr/bin/env python3
"""Shared painting kit for the environment surface PNGs.

Pure stdlib (no PIL, no numpy), byte-deterministic and importable by the
per-theme art modules (``office_art.py``, ``pool_art.py``) and by
``build.py`` itself.  The noise helpers are an approximate port of the
texture functions that used to live in ``src/render.rs``; they wrap at a
period so every sheet tiles seamlessly.
"""

from __future__ import annotations

import math
import struct
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

_MASK32 = 0xFFFFFFFF


def write_png(width: int, height: int, rgba: bytes) -> bytes:
    """Encodes 8-bit RGBA pixels as a PNG (filter 0, no interlacing)."""
    if len(rgba) != width * height * 4:
        raise ValueError("rgba buffer length does not match dimensions")
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)  # filter type: None
        raw += rgba[y * stride : (y + 1) * stride]

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + tag
            + payload
            + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        PNG_SIGNATURE
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def hash01(x: int, y: int, seed: int) -> float:
    """Deterministic per-texel hash in [0, 1), matching the renderer's mix."""
    h = ((x & _MASK32) * 0x9E3779B9) & _MASK32
    h ^= ((y & _MASK32) * 0x85EBCA6B) & _MASK32
    h ^= ((seed & _MASK32) * 0xC2B2AE35) & _MASK32
    h &= _MASK32
    h ^= h >> 15
    h = (h * 0x2545F491) & _MASK32
    h ^= h >> 13
    h = (h * 0x27D4EB2D) & _MASK32
    h ^= h >> 16
    return (h & 0x00FFFFFF) / 16777216.0


def tile_noise(x: int, y: int, size: int, period: int, seed: int) -> float:
    """Tileable value noise in [0, 1] over a ``size`` square, wrapped at ``period``."""
    period = max(1, period)
    scale = period / float(size)
    fx = x * scale
    fy = y * scale
    x0 = math.floor(fx)
    y0 = math.floor(fy)
    tx = fx - x0
    ty = fy - y0
    sx = tx * tx * (3.0 - 2.0 * tx)
    sy = ty * ty * (3.0 - 2.0 * ty)
    ix0 = int(x0) % period
    iy0 = int(y0) % period
    ix1 = (int(x0) + 1) % period
    iy1 = (int(y0) + 1) % period
    v00 = hash01(ix0, iy0, seed)
    v10 = hash01(ix1, iy0, seed)
    v01 = hash01(ix0, iy1, seed)
    v11 = hash01(ix1, iy1, seed)
    top = v00 + (v10 - v00) * sx
    bottom = v01 + (v11 - v01) * sx
    return top + (bottom - top) * sy


def tile_noise2(x: int, y: int, size: int, coarse: int, fine: int, seed: int) -> float:
    """Two octaves of tileable noise, the shape most of the surface ageing uses."""
    value = 0.65 * tile_noise(x, y, size, coarse, seed) + 0.35 * tile_noise(
        x, y, size, fine, seed + 7
    )
    return max(0.0, min(1.0, value))


def fbm(x: int, y: int, size: int, coarse: int, fine: int, finer: int, seed: int) -> float:
    """Three octaves of tileable noise for surfaces that need broader variation."""
    value = (
        0.5 * tile_noise(x, y, size, coarse, seed)
        + 0.33 * tile_noise(x, y, size, fine, seed + 11)
        + 0.17 * tile_noise(x, y, size, finer, seed + 23)
    )
    return max(0.0, min(1.0, value))


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


def smoothstep(value: float, low: float, high: float) -> float:
    """Hermite ramp between ``low`` and ``high``; flat outside the range."""
    if high <= low:
        return 0.0 if value < low else 1.0
    t = clamp((value - low) / (high - low), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def mix_rgb(a, b, amount: float):
    """Linear blend between two RGB triples, ``amount`` 0..1 towards ``b``."""
    amount = clamp(amount, 0.0, 1.0)
    return tuple(a[i] * (1.0 - amount) + b[i] * amount for i in range(3))


def scaled(rgb, factor: float):
    """Channel-wise multiplier that keeps the result non-negative."""
    return tuple(max(0.0, channel * factor) for channel in rgb)


class Canvas:
    """An 8-bit RGBA buffer with the handful of painting operations we need."""

    def __init__(self, width: int, height: int, fill: tuple[int, int, int, int] = (255, 255, 255, 255)) -> None:
        self.width = width
        self.height = height
        self.pixels = bytearray(bytes(fill) * (width * height))

    def set(self, x: int, y: int, rgb: tuple[float, float, float], alpha: int = 255) -> None:
        if x < 0 or y < 0 or x >= self.width or y >= self.height:
            return
        index = (y * self.width + x) * 4
        self.pixels[index] = int(round(clamp(rgb[0], 0.0, 255.0)))
        self.pixels[index + 1] = int(round(clamp(rgb[1], 0.0, 255.0)))
        self.pixels[index + 2] = int(round(clamp(rgb[2], 0.0, 255.0)))
        self.pixels[index + 3] = int(round(clamp(float(alpha), 0.0, 255.0)))

    def shade(self, x: int, y: int, factor: float, alpha: int = 255) -> None:
        """Multiplies an existing texel in place (used for grout, seams, wear)."""
        if x < 0 or y < 0 or x >= self.width or y >= self.height:
            return
        index = (y * self.width + x) * 4
        for channel in range(3):
            self.pixels[index + channel] = int(
                round(clamp(self.pixels[index + channel] * factor, 0.0, 255.0))
            )
        if alpha != 255:
            self.pixels[index + 3] = alpha

    def rect(self, x0: int, y0: int, x1: int, y1: int, rgb, alpha: int = 255) -> None:
        """Filled inclusive rectangle, clipped to the canvas."""
        for y in range(max(0, y0), min(self.height - 1, y1) + 1):
            for x in range(max(0, x0), min(self.width - 1, x1) + 1):
                self.set(x, y, rgb, alpha)

    def disc(self, cx: int, cy: int, radius: int, rgb, alpha: int = 255) -> None:
        """Filled circle; alpha 0 acts as an eraser when repainting RGBA."""
        limit = radius * radius + 0.5
        for y in range(cy - radius, cy + radius + 1):
            for x in range(cx - radius, cx + radius + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= limit:
                    self.set(x, y, rgb, alpha)

    def triangle_up(self, cx: int, apex_y: int, base_y: int, half_width: int, rgb, alpha: int = 255) -> None:
        """Up-pointing triangle: apex on top, widening towards ``base_y``."""
        span = max(1, base_y - apex_y)
        for y in range(apex_y, base_y + 1):
            half = int(round((y - apex_y) / span * half_width))
            for x in range(cx - half, cx + half + 1):
                self.set(x, y, rgb, alpha)

    def triangle_right(self, tip_x: int, base_x: int, cy: int, half_height: int, rgb, alpha: int = 255) -> None:
        """Right-pointing triangle: base on the left, tip at ``tip_x``."""
        span = max(1, tip_x - base_x)
        for x in range(base_x, tip_x + 1):
            half = int(round((x - base_x) / span * half_height))
            for y in range(cy - half, cy + half + 1):
                self.set(x, y, rgb, alpha)

    def rgba(self) -> bytes:
        return bytes(self.pixels)
