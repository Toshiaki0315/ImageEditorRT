"""プリセットの読み書きの期待値を、旧版の core/presets.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/presets/make.py

- saved.json: 旧版の save_presets で書いたファイル（Rust で同じ一覧を書くと同じバイト列になる）
- *.json（saved 以外）: 読み込みのテスト用のファイル（壊れた項目・壊れたファイルを含む）
- expected.json: それぞれを旧版の load_presets で読んだ結果（読めなければ error）
"""

import json
from pathlib import Path

from image_editor.core.diorama import DioramaDirection
from image_editor.core.filters import FilterType
from image_editor.core.frames import FrameType
from image_editor.core.presets import (
    Preset,
    PresetError,
    _preset_to_dict,
    load_presets,
    save_presets,
)
from image_editor.core.shapes import ShapeType
from image_editor.core.text import TextFont, TextPosition, TextSettings

HERE = Path(__file__).parent

saved = [
    Preset(name="夕焼け", filter=FilterType.SEPIA, exposure=0.7, brightness=-10, contrast=15, temperature=5200,
           saturation=20, vignette=30, aging=10, sharpen=40, blur=0, denoise=5, frame=FrameType.POLAROID,
           shape=ShapeType.ROUNDED, corner_radius=25,
           text=TextSettings(text="2026 \"夏\"\n旅", font=TextFont.MINCHO, size=7.5, color=(10, 20, 30), opacity=80,
                             position=TextPosition.FRAME_MARGIN)),
    Preset(name="ジオラマ", diorama_blur=60, diorama_direction=DioramaDirection.VERTICAL, diorama_position=40,
           diorama_width=15, diorama_vivid=70, shape=ShapeType.CIRCLE, frame=FrameType.INSTAX_MINI),
    Preset(name="既定"),
]
save_presets(HERE / "saved.json", saved)

files = {
    "items": {"version": 1, "presets": [
        {"name": "  空白つき  ", "exposure": "1.5", "brightness": 12.9, "unknown": 1},
        {"name": "既定値だけ"},
        {"name": "知らないテイスト", "filter": "no_such_filter"},
        {"name": "bool の数", "contrast": True},
        {"name": "文字列の数", "vignette": "30"},
        {"name": "壊れた文字", "text": {"color": [1, 2]}},
        {"name": "文字の値", "text": {"text": "abc", "font": "GOTHIC_BOLD", "size": "12", "color": [-5, 300, 128],
                                    "opacity": "40", "position": "top_left"}},
        {"name": "小数の色", "text": {"color": [1.0, 2, 3]}},
        {"name": "知らないフォント", "text": {"font": "COMIC"}},
        {"name": "既定値だけ", "exposure": 3.0},
        {"name": ""},
        {"name": 5},
        "文字列の項目",
        {"name": "x" * 60, "frame": "polaroid", "shape": "circle", "diorama_direction": "vertical"},
        {"name": "bool の露出", "exposure": True},
        {"name": "null の露出", "exposure": None},
    ]},
    "not_json": "{ これは JSON ではない",
    "no_presets": {"version": 1},
    "presets_not_list": {"presets": {"name": "a"}},
    "top_list": [{"name": "a"}],
    "empty": {"presets": []},
}
expected = {}
for name, content in files.items():
    path = HERE / f"{name}.json"
    path.write_text(content if isinstance(content, str) else json.dumps(content, ensure_ascii=False), encoding="utf-8")
    try:
        expected[name] = {"presets": [_preset_to_dict(p) for p in load_presets(path)]}
    except PresetError as e:
        expected[name] = {"error": str(e).replace(str(path), "<path>")}
expected["saved"] = {"presets": [_preset_to_dict(p) for p in load_presets(HERE / "saved.json")]}
(HERE / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
for name, value in expected.items():
    print(name, value.get("error") or [p["name"] for p in value["presets"]])
