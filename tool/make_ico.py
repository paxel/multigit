#!/usr/bin/env python3
"""Writes packaging/windows/multigit.ico from assets/icon_256.png.

The icon holds the 256 px PNG as it is, which Windows Vista and later read,
so no image library is needed. Run it again when the icon changes:

    python3 tool/make_ico.py
"""
import struct
import sys
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def ico_bytes(png: bytes) -> bytes:
    """An icon file holding the 256 x 256 PNG `png` as its one image."""
    if png[:8] != PNG_SIGNATURE or struct.unpack(">II", png[16:24]) != (256, 256):
        raise ValueError("not a 256 x 256 PNG")
    # Header: reserved, type 1 (icon), one image. Entry: width and height 0
    # mean 256; no palette; 1 plane; 32 bits; the size and where it starts.
    header = struct.pack("<HHH", 0, 1, 1)
    entry = struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(png), 6 + 16)
    return header + entry + png


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    try:
        icon = ico_bytes((root / "assets/icon_256.png").read_bytes())
    except ValueError as error:
        sys.exit(f"assets/icon_256.png: {error}")
    out = root / "packaging/windows/multigit.ico"
    out.write_bytes(icon)
    print(out.relative_to(root))


if __name__ == "__main__":
    main()
