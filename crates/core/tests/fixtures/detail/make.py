"""ディテール（シャープ・ぼかし・ノイズ除去）の期待値を、旧版の core/effects.py・core/pipeline.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/detail/make.py
"""

import json
import random
from pathlib import Path

from PIL import Image

from image_editor.core import effects, pipeline
from image_editor.core.pipeline import EditSettings
from image_editor.core.transform import CropRect

HERE = Path(__file__).parent


def sample(width: int, height: int) -> Image.Image:
    """なめらかな色の変化に、ざらつき（ノイズ）と輪郭・半透明のある画像。"""
    rng = random.Random(5)
    image = Image.new("RGBA", (width, height))
    image.putdata([
        (
            min(255, (x * 255) // (width - 1) + rng.randrange(-12, 13) % 256 // 8),
            (y * 255) // (height - 1) if x < width // 2 else 255 - (y * 255) // (height - 1),
            (x * 37 + y * 91 + rng.randrange(0, 30)) % 256,
            255 if (x + y) % 7 else 100 + x % 100,
        )
        for y in range(height)
        for x in range(width)
    ])
    return image


source = sample(160, 120)
source.save(HERE / "source.png")
cases = []
for amount in (15, 50, 100):
    for effect in ("sharpen", "blur", "denoise"):
        file = f"{effect}_{amount}.png"
        getattr(effects, effect)(source, amount).save(HERE / file)
        cases.append({"effect": effect, "amount": amount, "file": file})
# プレビューでのシャープ（保存時の半径の下限を換算する）
effects.sharpen(source, 70, reference=90, output=600).save(HERE / "sharpen_preview.png")
cases.append({"effect": "sharpen", "amount": 70, "reference": 90, "output": 600, "file": "sharpen_preview.png"})
(HERE / "cases.json").write_text(json.dumps(cases, indent=1) + "\n")

# 処理の流れ: 保存（原寸）と、縮小プレビュー（縮小率 0.25、トリミング範囲あり）
settings = EditSettings(sharpen=60, blur=20, denoise=40, crop=CropRect(40, 20, 400, 360), width=300)
big = sample(640, 480)
big.save(HERE / "big.png")
pipeline.apply_edits(big, settings).save(HERE / "apply_edits.png")
preview, factor = pipeline.make_preview(big, 160)
preview.save(HERE / "preview_source.png")
pipeline.render_preview(preview, settings, factor).save(HERE / "render_preview.png")
(HERE / "flow.json").write_text(json.dumps({"factor": factor}) + "\n")
print(len(cases), "cases, factor", factor)
