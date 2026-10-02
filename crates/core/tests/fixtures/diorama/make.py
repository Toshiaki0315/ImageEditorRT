"""ジオラマ風の期待値を、旧版の core/diorama.py・core/pipeline.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/diorama/make.py
"""

import json
import random
from pathlib import Path

from PIL import Image

from image_editor.core import diorama, pipeline
from image_editor.core.diorama import DioramaDirection, DioramaSettings
from image_editor.core.pipeline import EditSettings
from image_editor.core.transform import CropRect

HERE = Path(__file__).parent


def sample(width: int, height: int) -> Image.Image:
    """細かい模様（ぼけが分かる）と色の変化・半透明のある画像。"""
    rng = random.Random(7)
    image = Image.new("RGBA", (width, height))
    image.putdata([
        (
            255 if (x // 3 + y // 3) % 2 else (x * 255) // (width - 1),
            (y * 255) // (height - 1),
            rng.randrange(256),
            255 if (x + y) % 9 else 120,
        )
        for y in range(height)
        for x in range(width)
    ])
    return image


source = sample(160, 120)
source.save(HERE / "source.png")
cases = []
for i, (blur, direction, position, width, vivid, area) in enumerate([
    (80, "horizontal", 50, 20, 30, None),
    (100, "vertical", 30, 10, 0, None),
    (40, "horizontal", 70, 40, 100, None),
    (60, "vertical", 50, 0, 60, None),
    (70, "horizontal", 40, 30, 50, (20, 10, 140, 100)),
    (90, "vertical", 60, 20, 20, (30, 0, 100, 120)),
]):
    settings = DioramaSettings(blur, DioramaDirection(direction), position, width, vivid)
    file = f"case_{i}.png"
    diorama.diorama(source, settings, area=area).save(HERE / file)
    cases.append({"blur": blur, "direction": direction, "position": position, "width": width,
                  "vivid": vivid, "area": area, "file": file})
(HERE / "cases.json").write_text(json.dumps(cases, indent=1) + "\n")

settings = EditSettings(diorama_blur=80, diorama_position=40, diorama_vivid=50, crop=CropRect(10, 10, 120, 90))
pipeline.apply_edits(source, settings).save(HERE / "apply_edits.png")
pipeline.render_preview(source, settings).save(HERE / "render_preview.png")
vertical = EditSettings(diorama_blur=60, diorama_direction=DioramaDirection.VERTICAL, diorama_position=30)
pipeline.apply_edits(source, vertical).save(HERE / "apply_edits_vertical.png")
print(len(cases), "cases")
