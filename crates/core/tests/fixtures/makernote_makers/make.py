"""主なメーカーの MakerNote を入れた EXIF（TIFF の部分）と、旧版で読んだ結果を作る。

旧版の仮想環境で動かす（旧版の tests/exif_samples.py で EXIF を組み立て、旧版の
core/exif_info.py＝exifread で読む）:
    cd ImageEditor && .venv/bin/python <このファイル>
"""

from __future__ import annotations

import json
import struct
import sys
from pathlib import Path

OLD = Path("/Users/nomura/01_project/ImageEditor")
sys.path.insert(0, str(OLD / "src"))
sys.path.insert(0, str(OLD / "tests"))

from exif_samples import MAKERNOTE_AT, apple_note, build_exif  # noqa: E402

from image_editor.core.exif_info import ExifGroup, read_exif_info  # noqa: E402

HERE = Path(__file__).parent
BYTE, ASCII, SHORT, LONG, RATIONAL, SBYTE, UNDEF, SSHORT, SLONG, SRATIONAL, FLOAT, DOUBLE, IFD = range(1, 14)
FORMATS = {BYTE: "B", SHORT: "H", LONG: "I", SBYTE: "b", UNDEF: "B", SSHORT: "h", SLONG: "i", FLOAT: "f", DOUBLE: "d", IFD: "I"}


def field(kind: int, values, order: str) -> tuple[int, int, bytes]:
    """(型, 個数, 値のバイト列)。values は数の並び・文字列・バイト列。"""
    if kind == ASCII:
        raw = values if isinstance(values, bytes) else values.encode() + b"\0"
        return kind, len(raw), raw
    if isinstance(values, bytes):
        return kind, len(values), values
    if kind in (RATIONAL, SRATIONAL):
        code = "I" if kind == RATIONAL else "i"
        flat = [n for pair in values for n in pair]
        return kind, len(values), struct.pack(f"{order}{len(flat)}{code}", *flat)
    return kind, len(values), struct.pack(f"{order}{len(values)}{FORMATS[kind]}", *values)


def ifd(entries, start: int, order: str = "<", base: int = 0, count: int | None = None) -> bytes:
    """IFD を作る。start は IFD を置く位置、値の位置は base を引いた値で書く。

    entries は (タグ, 型, 値)。値を (個数, 位置) の組にすると、その位置をそのまま書く（範囲外など）。
    """
    data_at = start + 2 + 12 * len(entries) + 4
    head = struct.pack(order + "H", len(entries) if count is None else count)
    data = b""
    for tag, kind, values in entries:
        if isinstance(values, tuple):
            n, at = values
            head += struct.pack(order + "HHII", tag, kind, n, at)
            continue
        kind, n, raw = field(kind, values, order)
        if len(raw) <= 4:
            head += struct.pack(order + "HHI", tag, kind, n)[:8] + raw.ljust(4, b"\0")
        else:
            head += struct.pack(order + "HHII", tag, kind, n, data_at + len(data) - base)
            data += raw + (b"\0" if len(raw) % 2 else b"")
    return head + struct.pack(order + "I", 0) + data


CAMERA_INFO = bytearray(700)
CAMERA_INFO[23] = 150
CAMERA_INFO[25] = 160
CAMERA_INFO[27] = 170
CAMERA_INFO[204:208] = struct.pack("<I", 101)
CAMERA_INFO[208:210] = struct.pack("<H", 5001)
CAMERA_INFO[443:447] = struct.pack("<I", 7001)
CAMERA_INFO[455:459] = struct.pack("<I", 102)
CAMERA_INFO[475:479] = struct.pack("<I", 8001)
CAMERA_INFO[487:491] = struct.pack("<I", 103)
CAMERA_INFO[652:656] = struct.pack("<I", 9001)
CAMERA_INFO[656:660] = struct.pack("<I", 9002)
CAMERA_INFO[664:668] = struct.pack("<I", 104)
CAMERA_INFO[668:672] = struct.pack("<I", 105)

CANON_ENTRIES = [
    (0x0001, SHORT, [26, 2, 0, 4, 0, 0, 0, 3, 1, 9, 0xFFFF, 0, 77, 0, 4, 1]),
    (0x0002, SHORT, [0, 50, 1000, 1000]),
    (0x0004, SHORT, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 150, 13, 14]),
    (0x0006, ASCII, "Canon EOS 5D Mark III"),
    (0x0007, ASCII, "Firmware Version 1.3.4"),
    (0x0009, ASCII, ""),
    (0x000C, LONG, [123456789]),
    (0x000D, UNDEF, bytes(CAMERA_INFO)),
    (0x0010, LONG, [0x3330000]),
    (0x0012, SHORT, [8, 9, 3, 5472, 3648, 100]),
    (0x0026, SSHORT, [10, -2, 1, -300]),
    (0x0093, SHORT, [4, 100, 2, 0]),
    (0x7777, SHORT, [5]),
]


def canon(model: str, entries=CANON_ENTRIES):
    return build_exif(make="Canon", model=model, maker_note=lambda at: ifd(entries, at), prefix=False)


def nikon_type2(marker: bytes = b"II*\0"):
    entries = [
        (0x0001, UNDEF, b"0210"),
        (0x0002, SHORT, [0, 200]),
        (0x0004, ASCII, "FINE  "),
        (0x000E, UNDEF, bytes([250, 1, 12, 0])),
        (0x0012, UNDEF, bytes([252, 1, 6, 0])),
        (0x0017, UNDEF, bytes([8, 1, 6, 0])),
        (0x0018, UNDEF, bytes([0, 1, 6, 0])),
        (0x001D, ASCII, "SERIAL1234567"),
        (0x0084, RATIONAL, [(180, 10), (550, 10), (35, 10), (56, 10)]),
        (0x00AA, ASCII, "Normal"),
        (0x9999, SRATIONAL, [(-3, 6), (5, 0)]),
    ]
    note = b"Nikon\0\x02\x10\0\0" + marker + struct.pack("<I", 8) + ifd(entries, 8)
    return build_exif(make="NIKON CORPORATION", model="NIKON D750", maker_note=note, prefix=False)


def nikon_type1():
    entries = [(0x0003, SHORT, [3]), (0x0004, SHORT, [2]), (0x0006, SHORT, [9]), (0x000A, RATIONAL, [(15, 10)])]
    note = lambda at: b"Nikon\0\x01\0" + ifd(entries, at + 8)  # noqa: E731
    return build_exif(make="NIKON", model="E990", maker_note=note, prefix=False)


def nikon_plain():
    entries = [(0x0002, SHORT, [0, 100]), (0x0004, ASCII, "NORMAL"), (0x0012, UNDEF, bytes([3, 1, 6, 0]))]
    return build_exif(make="NIKON", model="E995", maker_note=lambda at: ifd(entries, at), prefix=False)


def olympus(special=(3, 2, 1)):
    entries = [
        (0x0200, LONG, list(special)),
        (0x0201, SHORT, [2]),
        (0x0204, RATIONAL, [(100, 100)]),
        (0x0207, ASCII, "OLYMPUS FIRMWARE"),
        (0x0209, UNDEF, b"OLYMPUS DIGITAL CAMERA\0\0\0\0\0\0\0\0\0\0"),
        (0x1000, SRATIONAL, [(-7, 3)]),
    ]
    note = lambda at: b"OLYMP\0\x01\0" + ifd(entries, at + 8)  # noqa: E731
    exif = [(0x9286, b"ASCII\0\0\0hello")]
    return build_exif(make="OLYMPUS IMAGING CORP.", model="E-M5", exif=exif, maker_note=note, prefix=False)


def casio():
    entries = [(0x0001, SHORT, [2]), (0x0002, SHORT, [3]), (0x0003, SHORT, [2]), (0x0014, SHORT, [80])]
    return build_exif(make="CASIO COMPUTER CO.,LTD.", model="EX-Z750", maker_note=lambda at: ifd(entries, at), prefix=False)


def sony(header: bool):
    entries = [
        (0xB020, ASCII, "Standard"),
        (0xB025, LONG, [1]),
        (0xB041, SHORT, [3]),
        (0x2000, UNDEF, bytes(range(60))),
        (0x9999, SBYTE, [-1, -2, 3, 4, 5]),
    ]
    head = b"SONY DSC \0\0\0" if header else b""
    note = lambda at: head + ifd(entries, at + len(head))  # noqa: E731
    return build_exif(make="SONY", model="DSC-RX100", maker_note=note, prefix=False)


def fujifilm():
    entries = [
        (0x0000, UNDEF, b"0130"),
        (0x1000, ASCII, "NORMAL "),
        (0x1001, SHORT, [3]),
        (0x1010, SHORT, [1]),
        (0x9998, FLOAT, [1.5, 0.1]),
        (0x9999, DOUBLE, [1e16]),
    ]
    note = b"FUJIFILM" + struct.pack("<I", 12) + ifd(entries, 12)
    return build_exif(make="FUJIFILM", model="X-T4", maker_note=note, order=">", prefix=False)


def apple(order: str):
    entries = [(0x0001, ("long", [14])), (0x0008, ("rational", [(1, 2), (3, 4), (5, 6)])), (0x000A, ("short", [3]))]
    return build_exif(make="Apple", model="iPhone 15", maker_note=apple_note(entries), order=order, prefix=False)


def dji():
    entries = [(0x0001, ASCII, "DJI"), (0x0003, FLOAT, [1.25]), (0x0006, FLOAT, [-3.5]), (0x0009, FLOAT, [0.0])]
    return build_exif(make="DJI", model="FC3582", maker_note=ifd(entries, 0), prefix=False)


def odd_values():
    """表示の細かい規則（長い並びの省略・読めない文字・データの外・知らない型・1000 個以上）。"""
    entries = [
        (0x0003, SHORT, list(range(60))),
        (0x0008, SSHORT, [-5]),
        (0x0009, ASCII, b"bad \xff\xfe utf8\0"),
        (0x000E, LONG, (3, 0x7FFFFFF0)),
        (0x0015, 99, (1, 5)),
        (0x0016, IFD, [8]),
        (0x0017, SHORT, (1200, 100)),
        (0x0018, ASCII, b"b\x80" * 40 + b"\0"),
        (0x0019, SLONG, [-1, 2]),
        (0x001A, RATIONAL, [(0, 0)]),
        (0x0028, ASCII, "x" * 400),
    ]
    return build_exif(make="Canon", model="Canon PowerShot", maker_note=lambda at: ifd(entries, at), prefix=False)


CASES = {
    "canon_5d3": lambda: canon("Canon EOS 5D Mark III"),
    "canon_5d": lambda: canon("Canon EOS 5D"),
    "canon_5d2": lambda: canon("Canon EOS 5D Mark II"),
    "canon_600d": lambda: canon("Canon EOS 600D"),
    "canon_other": lambda: canon("Canon EOS R5"),
    "canon_odd": odd_values,
    "nikon_type2": nikon_type2,
    "nikon_type2_no_marker": lambda: nikon_type2(b"II+\0"),
    "nikon_type1": nikon_type1,
    "nikon_plain": nikon_plain,
    "olympus": olympus,
    "olympus_broken": lambda: olympus((3, 2)),
    "casio": casio,
    "sony": lambda: sony(False),
    "sony_header": lambda: sony(True),
    "fujifilm": fujifilm,
    "apple": lambda: apple(">"),
    "apple_intel": lambda: apple("<"),
    "dji": dji,
}


def main() -> None:
    expected = {}
    for name, make in CASES.items():
        tiff = make()
        assert tiff[:2] in (b"II", b"MM") and len(tiff) >= MAKERNOTE_AT
        (HERE / f"{name}.tiff").write_bytes(tiff)
        info = read_exif_info(b"Exif\0\0" + tiff)
        expected[name] = {
            "maker_note": info.maker_note,
            "tags": [[e.tag, e.value] for e in info.entries if e.group is ExifGroup.MAKERNOTE],
            "user_comment": any(e.tag == "UserComment" for e in info.entries),
        }
    (HERE / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=1) + "\n")
    for name, value in expected.items():
        print(name, value["maker_note"], len(value["tags"]), value["tags"][:4])


if __name__ == "__main__":
    main()
