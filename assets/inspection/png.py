"""Read-only decoding of the checked-in, non-interlaced PNG assets."""
import zlib


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
