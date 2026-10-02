"""テイスト（フィルター 23 種）の期待値を、旧版の core/filters.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/filters/make.py
"""

import json
from pathlib import Path

from PIL import Image

from image_editor.core.filters import FilterType, apply_filter

HERE = Path(__file__).parent


def sample(width: int, height: int) -> Image.Image:
    """なめらかな色の変化と、細かい模様・半透明のある画像。"""
    image = Image.new("RGBA", (width, height))
    image.putdata([
        (
            (x * 255) // (width - 1),
            (y * 255) // (height - 1),
            (x * 37 + y * 91) % 256,
            255 if (x + y) % 7 else 100 + x,
        )
        for y in range(height)
        for x in range(width)
    ])
    return image


source = sample(64, 48)
source.save(HERE / "source.png")
big = sample(160, 120)
big.save(HERE / "big.png")
cases = []
for filter_type in FilterType:
    for name, image in (("source", source), ("big", big)):
        file = f"{filter_type.value}_{name}.png"
        apply_filter(image, filter_type).save(HERE / file)
        cases.append({"filter": filter_type.value, "label": filter_type.label, "source": f"{name}.png", "file": file})
(HERE / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1) + "\n")
print(len(cases), "cases")
