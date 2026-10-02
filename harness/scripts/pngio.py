"""Minimal PNG reading (no dependencies) and Game Boy frame conversion.

Reads 8-bit greyscale / RGB / RGBA / paletted PNGs, non-interlaced — enough
for the reference screenshots shipped with Mealybug Tearoom and cgb-acid2.

Frame formats used across the harness (identical to gb-cli and the SameBoy
runner):
  DMG: 160*144 bytes, shade 0..3 per pixel (0 = lightest)
  CGB: 160*144 little-endian u16, RGB555 = r | g << 5 | b << 10
Both are hashed with FNV-1a-64 over those bytes.
"""
import struct
import zlib

W, H = 160, 144


def read_png(data: bytes):
    """Return (width, height, channels, rows) with rows as lists of channel tuples."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, plte = 8, b"", None
    width = height = depth = ctype = interlace = None
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        t = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + n]
        if t == b"IHDR":
            width, height, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif t == b"PLTE":
            plte = [tuple(body[i:i + 3]) for i in range(0, len(body), 3)]
        elif t == b"IDAT":
            idat += body
        elif t == b"IEND":
            break
        pos += 12 + n
    if interlace:
        raise ValueError("interlaced PNG not supported")
    if depth != 8 and not (ctype in (0, 3) and depth in (1, 2, 4, 8)):
        raise ValueError(f"unsupported bit depth {depth} for colour type {ctype}")
    chans = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    bpp = max(1, chans * depth // 8)
    stride = (width * chans * depth + 7) // 8
    raw = zlib.decompress(idat)
    rows, prev = [], bytearray(stride)
    i = 0
    for _ in range(height):
        f = raw[i]
        line = bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 0xFF
            elif f == 2:
                line[x] = (line[x] + b) & 0xFF
            elif f == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 0xFF
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 0xFF
        prev = line
        if ctype == 0 and depth < 8:
            per, mask = 8 // depth, (1 << depth) - 1
            vals = [(line[k // per] >> (8 - depth * (k % per + 1))) & mask for k in range(width)]
            rows.append([(v * 255 // mask,) for v in vals])
        elif ctype == 3:
            if depth == 8:
                idx = list(line[:width])
            else:
                per = 8 // depth
                mask = (1 << depth) - 1
                idx = [(line[k // per] >> (8 - depth * (k % per + 1))) & mask for k in range(width)]
            rows.append([plte[j] for j in idx])
        else:
            rows.append([tuple(line[k * chans:(k + 1) * chans]) for k in range(width)])
    out_ch = 3 if ctype == 3 else chans
    return width, height, out_ch, rows


def png_to_frame(data: bytes, cgb: bool) -> bytes:
    """Convert a 160x144 reference screenshot to the harness frame format."""
    w, h, ch, rows = read_png(data)
    if (w, h) != (W, H):
        raise ValueError(f"expected 160x144, got {w}x{h}")
    out = bytearray()
    for row in rows:
        for px in row:
            if cgb:
                r, g, b = (px[0], px[1], px[2]) if ch >= 3 else (px[0],) * 3
                v = (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)
                out += bytes((v & 0xFF, v >> 8))
            else:
                grey = px[0] if ch in (1, 2) else px[0]   # DMG references are grey: 00 55 AA FF
                out.append(3 - min(3, grey * 4 // 256))
    return bytes(out)


def fnv1a64(b: bytes) -> int:
    h = 0xCBF29CE484222325
    for byte in b:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def ppm_to_frame(data: bytes) -> bytes:
    """Read a gb-cli / runner P6 dump (8-bit expanded channels) back to RGB555 LE."""
    body = data[-W * H * 3:]
    out = bytearray()
    for i in range(W * H):
        r, g, b = body[3 * i], body[3 * i + 1], body[3 * i + 2]
        v = (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)
        out += bytes((v & 0xFF, v >> 8))
    return bytes(out)


def pgm_to_frame(data: bytes) -> bytes:
    body = data[-W * H:]
    return bytes(3 - min(3, v * 4 // 256) for v in body)
