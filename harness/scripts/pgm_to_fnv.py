#!/usr/bin/env python3
"""Convert a grayscale PGM frame dump into the FNV-1a-64 hash gb-cli prints.

The reference emulator writes 160x144 grayscale with four distinct shades.
We quantise to 0..3 (0 = lightest) so the hash is palette-independent and
matches `gb_core::util::fnv1a64(framebuffer)`.
"""
import sys


def read_pgm(path):
    with open(path, "rb") as f:
        data = f.read()
    # Tokenise header: magic, width, height, maxval (comments allowed).
    tokens, i = [], 0
    while len(tokens) < 4:
        while data[i : i + 1].isspace():
            i += 1
        if data[i : i + 1] == b"#":
            while data[i : i + 1] not in (b"\n", b""):
                i += 1
            continue
        j = i
        while not data[j : j + 1].isspace():
            j += 1
        tokens.append(data[i:j])
        i = j
    i += 1  # single whitespace after maxval
    magic, w, h, maxval = tokens[0], int(tokens[1]), int(tokens[2]), int(tokens[3])
    if magic != b"P5":
        sys.exit(f"{path}: expected binary PGM (P5), got {magic!r}")
    pixels = data[i : i + w * h]
    if len(pixels) != w * h:
        sys.exit(f"{path}: truncated pixel data")
    return w, h, maxval, pixels


def quantise(pixels):
    levels = sorted(set(pixels), reverse=True)  # lightest first
    if len(levels) > 4:
        sys.exit(f"frame has {len(levels)} gray levels; expected <= 4")
    # Map each level to 0..3 by brightness rank; a frame using fewer than
    # four shades maps the ones present by absolute brightness bucket.
    def shade(v):
        return 3 - min(3, v * 4 // 256)
    return bytes(shade(v) for v in pixels)


def fnv1a64(b):
    h = 0xCBF29CE484222325
    for byte in b:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: pgm_to_fnv.py FRAME.pgm")
    w, h, _, px = read_pgm(sys.argv[1])
    if (w, h) != (160, 144):
        sys.exit(f"expected 160x144, got {w}x{h}")
    print(f"{fnv1a64(quantise(px)):016x}")
