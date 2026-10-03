"""ヒストグラムの期待値を、旧版の core/pipeline.py（render_preview_with_histogram）で作る。

画像は frame/ の source.png・opaque.png を使う。
    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/histogram/make.py
"""

import json
from pathlib import Path

from PIL import Image

from image_editor.core import pipeline
from image_editor.core.filters import FilterType
from image_editor.core.frames import FrameType
from image_editor.core.pipeline import EditSettings
from image_editor.core.shapes import ShapeType
from image_editor.core.text import TextSettings
from image_editor.core.transform import CropRect

HERE = Path(__file__).parent
FRAME = HERE.parent / "frame"

cases = []
for image, crop, frame, shape, radius, trimmed, extra in [
    ("source", None, "none", "rectangle", 0, False, {}),
    ("source", (10, 5, 60, 40), "none", "circle", 0, False, {}),
    ("source", (10, 5, 60, 40), "none", "circle", 0, True, {}),
    ("source", (5, 5, 70, 50), "polaroid", "rounded", 20, True, {}),
    ("source", (5, 5, 70, 50), "polaroid", "rounded", 20, False, {}),
    ("source", None, "none", "rectangle", 0, False, {"text": "ABC", "filter": "sepia", "vignette": 40}),
    ("opaque", None, "instax_mini", "rectangle", 0, False, {"exposure": 1.0}),
]:
    settings = EditSettings(
        crop=CropRect(*crop) if crop else None,
        frame=FrameType(frame),
        shape=ShapeType(shape),
        corner_radius=radius,
        filter=FilterType(extra.get("filter", "none")),
        vignette=extra.get("vignette", 0),
        exposure=extra.get("exposure", 0),
        text=TextSettings(text=extra.get("text", ""), size=20),
    )
    source = Image.open(FRAME / f"{image}.png").convert("RGBA")
    _, histogram = pipeline.render_preview_with_histogram(source, settings, trimmed=trimmed)
    cases.append({
        "image": image, "crop": crop, "frame": frame, "shape": shape, "radius": radius, "trimmed": trimmed,
        **extra,
        "red": list(histogram.red), "green": list(histogram.green), "blue": list(histogram.blue),
        "luma": list(histogram.luma),
    })

(HERE / "cases.json").write_text(json.dumps(cases) + "\n")
print(len(cases), [sum(c["luma"]) for c in cases])
