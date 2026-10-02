"""色の調整（露出〜経年劣化）の期待値を、旧版の core/effects.py・core/pipeline.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/adjust/make.py
"""

import json
from pathlib import Path

from PIL import Image

from image_editor.core import effects, pipeline
from image_editor.core.pipeline import EditSettings
from image_editor.core.transform import CropRect

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
tall = sample(37, 91)
tall.save(HERE / "tall.png")

cases = []


def case(name: str, image: Image.Image, source_name: str, effect: str, amount) -> None:
    image.save(HERE / f"{name}.png")
    cases.append({"file": f"{name}.png", "source": source_name, "effect": effect, "amount": amount})


for ev in (-5.0, -1.3, 0.7, 2.5, 5.0):
    case(f"exposure_{ev}", effects.exposure(source, ev), "source.png", "exposure", ev)
for amount in (-100, -37, 50, 100):
    case(f"brightness_{amount}", effects.brightness(source, amount), "source.png", "brightness", amount)
for amount in (-100, -40, 30, 100):
    case(f"contrast_{amount}", effects.contrast(source, amount), "source.png", "contrast", amount)
for kelvin in (2000, 3300, 5000, 7700, 10000):
    case(f"temperature_{kelvin}", effects.color_temperature(source, kelvin), "source.png", "temperature", kelvin)
for amount in (-100, -50, 40, 100):
    case(f"saturation_{amount}", effects.saturation(source, amount), "source.png", "saturation", amount)
for amount in (30, 100):
    case(f"vignette_{amount}", effects.vignette(source, amount), "source.png", "vignette", amount)
    case(f"vignette_tall_{amount}", effects.vignette(tall, amount), "tall.png", "vignette", amount)
for amount in (25, 60, 100):
    case(f"aging_{amount}", effects.aging(source, amount), "source.png", "aging", amount)
    case(f"aging_tall_{amount}", effects.aging(tall, amount), "tall.png", "aging", amount)

(HERE / "cases.json").write_text(json.dumps(cases, indent=1) + "\n")

# 色の調整をまとめてかける（保存の流れ）と、トリミング範囲のあるプレビュー
combined = EditSettings(
    exposure=0.6, brightness=15, contrast=25, temperature=4800, saturation=30, vignette=40, aging=20
)
pipeline.apply_edits(source, combined).save(HERE / "apply_edits.png")
cropped = EditSettings(vignette=70, aging=10, saturation=-20, crop=CropRect(8, 6, 40, 30))
pipeline.render_preview(source, cropped).save(HERE / "render_preview.png")
print(len(cases), "cases")
