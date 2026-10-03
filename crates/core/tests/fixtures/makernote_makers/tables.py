"""exifread の MakerNote のタグ表から、Rust のタグ表（crates/core/src/exifread_tables.rs）を作る。

旧版の仮想環境で動かす:
    cd ImageEditor && .venv/bin/python <このファイル>
"""

from __future__ import annotations

import json
from pathlib import Path

import exifread
from exifread.tags.makernote import apple, canon, casio, dji, fujifilm, nikon, olympus, sony

OUT = Path(__file__).resolve().parents[3] / "src" / "exifread_tables.rs"
FUNCS = {"ev_bias": "EvBias", "make_string": "MakeString", "special_mode": "SpecialMode", "convert_temp": "ConvertTemp"}


def literal(text: str) -> str:
    assert all(ord(c) >= 0x20 for c in text), text
    return json.dumps(text, ensure_ascii=False)


def table(name: str, tags: dict) -> str:
    rows = []
    for tag in sorted(tags):
        entry = tags[tag]
        assert len(entry) == 2 and 0 <= tag <= 0xFFFF and entry[0] != "UNDEF", (name, tag, entry)
        kind = entry[1]
        if kind is None:
            fmt = "Plain"
        elif callable(kind):
            fmt = f"Func(Func::{FUNCS[kind.__name__]})"
        else:
            assert isinstance(kind, dict), (name, tag)
            assert all(isinstance(k, int) for k in kind), (name, tag)
            pairs = ", ".join(f"({k}, {literal(v)})" for k, v in sorted(kind.items()))
            fmt = f"Map(&[{pairs}])"
        rows.append(f"    (0x{tag:04X}, {literal(entry[0])}, {fmt}),")
    return f"pub const {name}: &[TagDef] = &[\n" + "\n".join(rows) + "\n];\n"


def main() -> None:
    parts = [
        "//! exifread "
        + exifread.__version__
        + " の MakerNote のタグ表（名前と値の表示）。\n"
        "//! tests/fixtures/makernote_makers/tables.py で作ったもの。手で直さない。\n\n"
        "use crate::exifread_note::{Format::*, Func, TagDef};\n",
    ]
    for name, tags in [
        ("NIKON_OLD", nikon.TAGS_OLD),
        ("NIKON_NEW", nikon.TAGS_NEW),
        ("OLYMPUS", olympus.TAGS),
        ("CASIO", casio.TAGS),
        ("SONY", sony.TAGS),
        ("FUJIFILM", fujifilm.TAGS),
        ("APPLE", apple.TAGS),
        ("DJI", dji.TAGS),
        ("CANON", canon.TAGS),
    ]:
        parts.append(table(name, tags))
    offsets = []
    for tag, tags in canon.OFFSET_TAGS.items():
        assert f"Tag 0x{tag:04X}" not in [v[0] for v in canon.TAGS.values()] and tag not in canon.TAGS
        parts.append(table(f"CANON_0X{tag:04X}", tags))
        offsets.append(f"    (0x{tag:04X}, CANON_0X{tag:04X}),")
    assert 0x000D not in canon.TAGS
    parts.append(
        "/// Canon の、値の位置ごとに意味のあるタグ（exifread の OFFSET_TAGS と同じ順）。\n"
        "pub const CANON_OFFSET_TAGS: &[(u16, &[TagDef])] = &[\n" + "\n".join(offsets) + "\n];\n"
    )
    OUT.write_text("\n".join(parts))
    print(OUT, OUT.stat().st_size)


if __name__ == "__main__":
    main()
