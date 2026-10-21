#!/usr/bin/env python3
"""Собирает app/res/zvukopad.ico из большой картинки PNG (квадратной, с прозрачностью).

    python tools/make_ico.py assets/icon/rizhik-head-1024.png

Нужна библиотека Pillow (pip install pillow). Маленькие размеры пишутся как обычные
32-битные BMP (их понимает любая Windows), 256×256 — как PNG (так делает сама Windows).
Можно и без этого скрипта: положите любой свой .ico в app/res/zvukopad.ico.
"""
import io
import struct
import sys

from PIL import Image

SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def bmp_entry(img):
    s = img.width
    px = img.convert("RGBA").tobytes("raw", "BGRA")
    rows = [px[y * s * 4:(y + 1) * s * 4] for y in range(s)]
    xor = b"".join(reversed(rows))  # снизу вверх
    mask_row = ((s + 31) // 32) * 4
    andmask = b"\0" * (mask_row * s)
    header = struct.pack("<IiiHHIIiiII", 40, s, s * 2, 1, 32, 0, len(xor) + len(andmask), 0, 0, 0, 0)
    return header + xor + andmask


def main():
    src = Image.open(sys.argv[1]).convert("RGBA")
    out = sys.argv[2] if len(sys.argv) > 2 else "app/res/zvukopad.ico"
    blobs = []
    for s in SIZES:
        im = src.resize((s, s), Image.LANCZOS)
        if s >= 256:
            b = io.BytesIO()
            im.save(b, "PNG", optimize=True)
            blobs.append((s, b.getvalue()))
        else:
            blobs.append((s, bmp_entry(im)))
    head = struct.pack("<HHH", 0, 1, len(blobs))
    offset = 6 + 16 * len(blobs)
    dirs, data = b"", b""
    for s, blob in blobs:
        dim = 0 if s >= 256 else s
        dirs += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(blob), offset + len(data))
        data += blob
    with open(out, "wb") as f:
        f.write(head + dirs + data)
    print(f"{out}: {len(blobs)} размеров, {len(head) + len(dirs) + len(data)} байт")


if __name__ == "__main__":
    main()
