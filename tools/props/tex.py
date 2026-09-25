"""Tiny procedural texture painter for the places prop pack.

Design goals (see ``tools/props/README.md``): 32x32 to 256x256 RGBA canvases
(256 is the normal native prop-atlas size), no external dependencies (pure
stdlib + zlib), deterministic output, and painting operations that stay legible
at the game's authored UI reference size. Places is a desktop game: nothing
here is limited by a handheld memory budget.

Textures are organised into named *regions*: a region is a pixel rectangle on
the canvas (usually one cell of an ``auto`` grid) that a mesh face references
through :meth:`Texture.uv`.  Regions with names such as ``front``, ``side`` or
``top`` keep one prop's texture readable without needing a texture atlas.
"""

from __future__ import annotations

import struct
import zlib
from typing import Dict, Iterable, Sequence

from palette import shade

Color = tuple[int, int, int]
Rect = tuple[int, int, int, int]  # x, y, width, height in pixels


class Rng:
    """Small deterministic PRNG (splitmix64) so output never drifts by platform."""

    def __init__(self, seed: int = 1) -> None:
        self._state = (seed * 0x9E3779B97F4A7C15 + 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF

    def next_u64(self) -> int:
        self._state = (self._state + 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
        z = self._state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
        return z ^ (z >> 31)

    def uniform(self, low: float = 0.0, high: float = 1.0) -> float:
        return low + (high - low) * (self.next_u64() / 0xFFFFFFFFFFFFFFFF)

    def randint(self, low: int, high: int) -> int:
        """Inclusive integer range."""
        if high <= low:
            return low
        return low + int(self.next_u64() % (high - low + 1))

    def chance(self, probability: float) -> bool:
        return self.uniform() < probability

    def pick(self, items: Sequence):
        return items[int(self.next_u64() % len(items))]


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
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


class Texture:
    """A procedural RGBA canvas with named UV regions."""

    def __init__(self, size: int = 64, seed: int = 1) -> None:
        if size not in (32, 64, 128, 256):
            raise ValueError("texture size must be 32, 64, 128 or 256")
        self.width = size
        self.height = size
        self.pixels = bytearray(size * size * 4)
        self.regions: Dict[str, Rect] = {}
        self.rng = Rng(seed)
        self._last_fill: Dict[str, Color] = {}

    # --------------------------------------------------------------- regions

    def auto(self, *names: str, cols: int | None = None, rows: int | None = None) -> Dict[str, Rect]:
        """Splits the canvas into equal cells and registers one region per name.

        Cells are laid out left-to-right, top-to-bottom.  With two names the
        canvas is split into a 2x1 or 1x2 grid (vertical halves by default) --
        the usual "front / side" layout for a simple prop.
        """
        count = len(names)
        if count == 0:
            return {}
        if cols is None and rows is None:
            if count <= 2:
                cols, rows = 1, count
            elif count <= 4:
                cols, rows = 2, (count + 1) // 2
            else:
                cols = 4
                rows = (count + 3) // 4
        elif cols is None:
            cols = (count + rows - 1) // rows  # type: ignore[operator]
        elif rows is None:
            rows = (count + cols - 1) // cols  # type: ignore[operator]

        cell_w = self.width // cols
        cell_h = self.height // rows
        assert cell_w > 0 and cell_h > 0, "too many regions for this texture size"

        out: Dict[str, Rect] = {}
        for index, name in enumerate(names):
            cx = index % cols
            cy = index // cols
            rect = (cx * cell_w, cy * cell_h, cell_w, cell_h)
            self.regions[name] = rect
            out[name] = rect
        return out

    def region(self, name: str, rect: Rect) -> Rect:
        self.regions[name] = rect
        return rect

    def cell(self, name: str) -> Rect:
        return self.regions[name]

    def uv(self, name: str, inset: float = 0.0) -> tuple[float, float, float, float]:
        """Region as ``(u0, v0, u1, v1)`` with an optional half-texel inset."""
        x, y, w, h = self.regions[name]
        pad = inset
        return (
            (x + pad) / self.width,
            (y + pad) / self.height,
            (x + w - pad) / self.width,
            (y + h - pad) / self.height,
        )

    def sub(self, name: str, fx0: float, fy0: float, fx1: float, fy1: float) -> tuple[float, float, float, float]:
        """Fractional sub-rectangle of a region, returned as UVs."""
        x, y, w, h = self.regions[name]
        u0, v0, _, _ = self.uv(name)
        return (
            u0 + fx0 * w / self.width,
            v0 + fy0 * h / self.height,
            u0 + fx1 * w / self.width,
            v0 + fy1 * h / self.height,
        )

    # ---------------------------------------------------------------- pixels

    def _put(self, x: int, y: int, color: Color, alpha: int = 255, blend: float = 1.0) -> None:
        if x < 0 or y < 0 or x >= self.width or y >= self.height:
            return
        index = (y * self.width + x) * 4
        if blend >= 1.0 and alpha >= 255:
            self.pixels[index] = max(0, min(255, int(round(color[0]))))
            self.pixels[index + 1] = max(0, min(255, int(round(color[1]))))
            self.pixels[index + 2] = max(0, min(255, int(round(color[2]))))
            self.pixels[index + 3] = 255
            return
        # `alpha` is opacity (0..255) and `blend` an additional weight, so a
        # translucent wear mark softens while a 255-alpha button stays solid.
        t = max(0.0, min(1.0, blend * (alpha / 255.0)))
        for channel in range(3):
            old = self.pixels[index + channel]
            self.pixels[index + channel] = max(0, min(255, int(round(old + (color[channel] - old) * t))))
        self.pixels[index + 3] = 255

    def _rect_pixels(self, name: str, sub: tuple[float, float, float, float] | None = None):
        x, y, w, h = self.regions[name]
        if sub is not None:
            fx0, fy0, fx1, fy1 = sub
            sx0 = int(round(fx0 * w))
            sy0 = int(round(fy0 * h))
            sx1 = int(round(fx1 * w))
            sy1 = int(round(fy1 * h))
            return x + min(sx0, sx1), y + min(sy0, sy1), max(1, abs(sx1 - sx0)), max(1, abs(sy1 - sy0))
        return x, y, w, h

    # ------------------------------------------------------------ operations

    def fill(self, name: str, color: Color, sub=None, jitter: int = 0, seed: int = 7) -> None:
        """Fills a region (or sub-rect) with a colour and optional per-pixel noise."""
        x, y, w, h = self._rect_pixels(name, sub)
        rng = Rng(seed + x * 31 + y * 17)
        for py in range(y, y + h):
            for px in range(x, x + w):
                if jitter:
                    delta = rng.randint(-jitter, jitter)
                    self._put(px, py, shade(color, 1.0 + delta / 255.0))
                else:
                    self._put(px, py, color)
        self._last_fill[name] = color

    def gradient(self, name: str, top: Color, bottom: Color, sub=None, jitter: int = 0, seed: int = 11) -> None:
        """Vertical gradient fill; ``top`` is the small-V (upper) end of the region."""
        x, y, w, h = self._rect_pixels(name, sub)
        rng = Rng(seed + x * 13 + y * 29)
        for py in range(y, y + h):
            t = (py - y) / max(1, h - 1)
            base = tuple(int(round(top[c] + (bottom[c] - top[c]) * t)) for c in range(3))
            for px in range(x, x + w):
                if jitter:
                    delta = rng.randint(-jitter, jitter)
                    self._put(px, py, shade(base, 1.0 + delta / 255.0))
                else:
                    self._put(px, py, base)

    def noise(self, name: str, amount: int = 8, freq: int = 2, seed: int = 3, sub=None) -> None:
        """Adds low-frequency value noise on top of the existing pixels."""
        x, y, w, h = self._rect_pixels(name, sub)
        rng = Rng(seed)
        cells = max(1, (max(w, h) + freq - 1) // freq + 1)
        grid = [[rng.uniform(-1.0, 1.0) for _ in range(cells)] for _ in range(cells)]
        for py in range(y, y + h):
            for px in range(x, x + w):
                gx = (px - x) / freq
                gy = (py - y) / freq
                ix, iy = int(gx), int(gy)
                fx, fy = gx - ix, gy - iy
                top = grid[iy][ix] * (1 - fx) + grid[iy][min(ix + 1, cells - 1)] * fx
                bot = grid[min(iy + 1, cells - 1)][ix] * (1 - fx) + grid[min(iy + 1, cells - 1)][min(ix + 1, cells - 1)] * fx
                value = (top * (1 - fy) + bot * fy) * amount
                index = (py * self.width + px) * 4
                for channel in range(3):
                    self.pixels[index + channel] = max(0, min(255, int(round(self.pixels[index + channel] + value))))

    def streaks(self, name: str, color: Color, count: int = 6, seed: int = 5, alpha: int = 40, direction: str = "v", sub=None) -> None:
        """Vertical or horizontal grime/wood-grain streaks."""
        x, y, w, h = self._rect_pixels(name, sub)
        rng = Rng(seed)
        for _ in range(count):
            shade_factor = rng.uniform(0.6, 1.4)
            length = rng.uniform(0.35, 1.0)
            if direction == "v":
                sx = x + rng.randint(0, max(0, w - 1))
                sy = y + int(rng.uniform(0.0, 1.0 - length) * h)
                count_px = max(2, int(length * h))
                for step in range(count_px):
                    fade = 1.0 - abs(step / count_px - 0.5) * 2.0
                    self._put(sx, sy + step, shade(color, shade_factor), alpha, blend=0.8 * fade + 0.2)
            else:
                sy = y + rng.randint(0, max(0, h - 1))
                sx = x + int(rng.uniform(0.0, 1.0 - length) * w)
                count_px = max(2, int(length * w))
                for step in range(count_px):
                    fade = 1.0 - abs(step / count_px - 0.5) * 2.0
                    self._put(sx + step, sy, shade(color, shade_factor), alpha, blend=0.8 * fade + 0.2)

    def grain(self, name: str, color: Color, seed: int = 9, density: float = 0.35, alpha: int = 30, sub=None) -> None:
        """Short horizontal dashes, used as a cheap wood-grain / brushed-metal read."""
        x, y, w, h = self._rect_pixels(name, sub)
        rng = Rng(seed)
        rows = max(2, h // 2)
        for row in range(rows):
            py = y + int(row * h / rows)
            cursor = 0
            while cursor < w:
                run = rng.randint(2, max(3, w // 3))
                if rng.chance(density):
                    tone = rng.uniform(0.75, 1.2)
                    for step in range(run):
                        self._put(x + cursor + step, py, shade(color, tone), alpha, blend=0.85)
                cursor += run + rng.randint(0, 2)

    def spots(self, name: str, color: Color, count: int = 10, seed: int = 4, radius: int = 2, alpha: int = 60, sub=None) -> None:
        """Soft blobs: stains, rust patches, wear."""
        x, y, w, h = self._rect_pixels(name, sub)
        rng = Rng(seed)
        for _ in range(count):
            cx = x + rng.randint(0, max(0, w - 1))
            cy = y + rng.randint(0, max(0, h - 1))
            r = max(1, radius + rng.randint(-1, 1))
            for py in range(cy - r, cy + r + 1):
                for px in range(cx - r, cx + r + 1):
                    dist = ((px - cx) ** 2 + (py - cy) ** 2) ** 0.5
                    if dist <= r:
                        self._put(px, py, shade(color, rng.uniform(0.8, 1.15)), alpha, blend=1.0 - dist / (r + 0.5))

    def border(self, name: str, color: Color, width: int = 1, sub=None, alpha: int = 90) -> None:
        """Darkens/lightens a rectangle outline: panel seams, box edges."""
        x, y, w, h = self._rect_pixels(name, sub)
        for px in range(x, x + w):
            for offset in range(width):
                self._put(px, y + offset, color, alpha, blend=0.9)
                self._put(px, y + h - 1 - offset, color, alpha, blend=0.9)
        for py in range(y, y + h):
            for offset in range(width):
                self._put(x + offset, py, color, alpha, blend=0.9)
                self._put(x + w - 1 - offset, py, color, alpha, blend=0.9)

    def band(self, name: str, color: Color, v0: float, v1: float, sub=None, alpha: int = 255, jitter: int = 0) -> None:
        """Horizontal band across a region, addressed in 0..1 fractions."""
        x, y, w, h = self._rect_pixels(name, sub)
        y0 = y + int(round(min(v0, v1) * h))
        y1 = y + int(round(max(v0, v1) * h))
        rng = Rng(int(v0 * 1000) + x)
        for py in range(max(y, y0), min(y + h, y1)):
            for px in range(x, x + w):
                if jitter:
                    self._put(px, py, shade(color, 1.0 + rng.randint(-jitter, jitter) / 255.0))
                else:
                    self._put(px, py, color, alpha, blend=0.95)

    def panel(self, name: str, color: Color, sub=None, rect: tuple[float, float, float, float] | None = None,
              depth: int = 2, alpha: int = 120) -> None:
        """Paints a recessed/flat panel with a highlight top edge and shadow bottom edge.

        ``rect`` is a fractional sub-rectangle of the region; when omitted the
        whole region is treated as one panel.
        """
        x, y, w, h = self._rect_pixels(name, sub)
        if rect is not None:
            fx0, fy0, fx1, fy1 = rect
            px0 = x + int(round(min(fx0, fx1) * w))
            py0 = y + int(round(min(fy0, fy1) * h))
            px1 = x + int(round(max(fx0, fx1) * w))
            py1 = y + int(round(max(fy0, fy1) * h))
        else:
            px0, py0, px1, py1 = x, y, x + w, y + h
        for py in range(py0, py1):
            for px in range(px0, px1):
                self._put(px, py, color, alpha, blend=0.55)
        for px in range(px0, min(px1, self.width)):
            for offset in range(depth):
                self._put(px, py0 + offset, shade(color, 1.25))
        for px in range(px0, min(px1, self.width)):
            for offset in range(depth):
                self._put(px, py1 - 1 - offset, shade(color, 0.6))
        for py in range(py0, py1):
            for offset in range(depth):
                self._put(px0 + offset, py, shade(color, 1.12))
                self._put(px1 - 1 - offset, py, shade(color, 0.68))

    def dots(self, name: str, color: Color, positions: Iterable[tuple[float, float]], radius: int = 1,
             sub=None, alpha: int = 255) -> None:
        """Small filled circles addressed in region fractions (buttons, rivets)."""
        x, y, w, h = self._rect_pixels(name, sub)
        for fx, fy in positions:
            cx = x + int(round(fx * (w - 1)))
            cy = y + int(round(fy * (h - 1)))
            for py in range(cy - radius, cy + radius + 1):
                for px in range(cx - radius, cx + radius + 1):
                    if (px - cx) ** 2 + (py - cy) ** 2 <= radius * radius + 0.5:
                        self._put(px, py, color, alpha)

    def bar(self, name: str, color: Color, rect: tuple[float, float, float, float], sub=None, alpha: int = 255) -> None:
        """Filled fractional rectangle (labels, slots, handles, screens)."""
        x, y, w, h = self._rect_pixels(name, sub)
        fx0, fy0, fx1, fy1 = rect
        px0 = x + int(round(min(fx0, fx1) * w))
        py0 = y + int(round(min(fy0, fy1) * h))
        px1 = x + int(round(max(fx0, fx1) * w))
        py1 = y + int(round(max(fy0, fy1) * h))
        for py in range(py0, py1):
            for px in range(px0, px1):
                self._put(px, py, color, alpha)

    def scribble(self, name: str, color: Color, rect: tuple[float, float, float, float], seed: int = 2,
                 sub=None, alpha: int = 200, text_blocks: int = 4) -> None:
        """Illegible blocky "text": a generic fictional label, never real branding."""
        rng = Rng(seed)
        x, y, w, h = self._rect_pixels(name, sub)
        fx0, fy0, fx1, fy1 = rect
        px0 = x + int(round(min(fx0, fx1) * w))
        py0 = y + int(round(min(fy0, fy1) * h))
        px1 = x + int(round(max(fx0, fx1) * w))
        py1 = y + int(round(max(fy0, fy1) * h))
        line_h = max(1, (py1 - py0) // max(1, text_blocks * 2 - 1))
        py = py0
        while py + line_h <= py1:
            cursor = px0
            while cursor < px1 - 1:
                word = rng.randint(2, max(3, (px1 - px0) // 3))
                for _ in range(min(word, px1 - cursor)):
                    self._put(cursor, py, color, alpha)
                    cursor += 1
                cursor += rng.randint(1, 3)
            py += line_h * 2

    # --------------------------------------------------------------- output

    def to_rgba(self) -> bytes:
        # Guarantee full opacity: props use no alpha blending.
        data = bytearray(self.pixels)
        for index in range(3, len(data), 4):
            data[index] = 255
        return bytes(data)

    def png_bytes(self) -> bytes:
        return write_png(self.width, self.height, self.to_rgba())

    def sample(self, u: float, v: float) -> Color:
        """Nearest-neighbour sample, used for baked vertex colours in previews."""
        x = min(self.width - 1, max(0, int(u * self.width)))
        y = min(self.height - 1, max(0, int(v * self.height)))
        index = (y * self.width + x) * 4
        return (self.pixels[index], self.pixels[index + 1], self.pixels[index + 2])


def hash_color(seed: int) -> Color:
    rng = Rng(seed)
    return (rng.randint(60, 210), rng.randint(60, 200), rng.randint(60, 190))


def decode_png(data: bytes) -> tuple[int, int, bytes]:
    """Decodes 8-bit non-interlaced PNG into RGBA bytes (grayscale/rgb/palette-safe subset)."""
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("not a PNG file")
    offset = 8
    width = height = 0
    bit_depth = color_type = 0
    interlace = 0
    palette: list[tuple[int, int, int]] = []
    raw = bytearray()
    while offset + 8 <= len(data):
        length = int.from_bytes(data[offset : offset + 4], "big")
        tag = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if tag == b"IHDR":
            width = int.from_bytes(payload[0:4], "big")
            height = int.from_bytes(payload[4:8], "big")
            bit_depth = payload[8]
            color_type = payload[9]
            interlace = payload[12]
        elif tag == b"PLTE":
            palette = [(payload[i], payload[i + 1], payload[i + 2]) for i in range(0, len(payload), 3)]
        elif tag == b"IDAT":
            raw += payload
        elif tag == b"IEND":
            break

    if bit_depth != 8 or interlace != 0:
        raise ValueError(f"unsupported PNG (bit depth {bit_depth}, interlace {interlace})")
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color_type]
    stride = width * channels
    pixels = bytearray(height * stride)
    stream = zlib.decompress(bytes(raw))
    previous = bytearray(stride)
    cursor = 0
    for y in range(height):
        filter_type = stream[cursor]
        cursor += 1
        line = bytearray(stream[cursor : cursor + stride])
        cursor += stride
        if filter_type == 1:
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
                left = line[i - channels] if i >= channels else 0
                up = previous[i]
                up_left = previous[i - channels] if i >= channels else 0
                estimate = left + up - up_left
                pa, pb, pc = abs(estimate - left), abs(estimate - up), abs(estimate - up_left)
                predictor = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[i] = (line[i] + predictor) & 0xFF
        elif filter_type != 0:
            raise ValueError(f"unknown PNG filter {filter_type}")
        pixels[y * stride : (y + 1) * stride] = line
        previous = line

    rgba = bytearray(width * height * 4)
    for index in range(width * height):
        if color_type == 6:
            rgba[index * 4 : index * 4 + 4] = pixels[index * 4 : index * 4 + 4]
        elif color_type == 2:
            rgba[index * 4 : index * 4 + 3] = pixels[index * 3 : index * 3 + 3]
            rgba[index * 4 + 3] = 255
        elif color_type == 0:
            value = pixels[index]
            rgba[index * 4 : index * 4 + 3] = bytes((value, value, value))
            rgba[index * 4 + 3] = 255
        elif color_type == 4:
            value = pixels[index * 2]
            rgba[index * 4 : index * 4 + 3] = bytes((value, value, value))
            rgba[index * 4 + 3] = pixels[index * 2 + 1]
        elif color_type == 3:
            r, g, b = palette[pixels[index]]
            rgba[index * 4 : index * 4 + 3] = bytes((r, g, b))
            rgba[index * 4 + 3] = 255
    return width, height, bytes(rgba)
