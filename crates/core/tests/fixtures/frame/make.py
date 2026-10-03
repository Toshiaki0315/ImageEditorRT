"""フレーム・形の期待値を、旧版の core/frames.py・core/shapes.py・core/pipeline.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/frame/make.py
"""

import json
from pathlib import Path

from PIL import Image

from image_editor.core import frames, pipeline, shapes
from image_editor.core.frames import FrameType
from image_editor.core.pipeline import EditSettings
from image_editor.core.shapes import ShapeType
from image_editor.core.transform import CropRect

HERE = Path(__file__).parent


def sample(width: int, height: int) -> Image.Image:
    image = Image.new("RGBA", (width, height))
    image.putdata([
        ((x * 255) // max(1, width - 1), (y * 255) // max(1, height - 1), (x * 31 + y * 17) % 256,
         255 if (x + y) % 11 else 90)
        for y in range(height) for x in range(width)
    ])
    return image


source = sample(90, 60)
source.save(HERE / "source.png")
tall = sample(50, 80)
tall.save(HERE / "tall.png")
opaque = sample(70, 50).convert("RGB").convert("RGBA")
opaque.save(HERE / "opaque.png")

masks = []
for size, shape, radius in [((90, 60), "rounded", 10), ((90, 60), "rounded", 50), ((90, 60), "rounded", 0),
                            ((90, 60), "circle", 10), ((51, 80), "circle", 10), ((33, 33), "rounded", 23),
                            ((200, 7), "rounded", 40), ((9, 9), "circle", 0)]:
    mask = shapes.shape_mask(size, ShapeType(shape), radius)
    file = None
    if mask is not None:
        file = f"mask_{shape}_{size[0]}x{size[1]}_{radius}.png"
        mask.save(HERE / file)
    masks.append({"size": size, "shape": shape, "radius": radius, "file": file})

images = []
def keep(name, image):
    image.save(HERE / name)
    images.append(name)

keep("shape_rounded_clear.png", shapes.apply_shape(source, ShapeType.ROUNDED, 20))
keep("shape_circle_fill.png", shapes.apply_shape(source, ShapeType.CIRCLE, 10, fill=(255, 255, 255)))
keep("shape_rounded_fill_opaque.png", shapes.apply_shape(opaque, ShapeType.ROUNDED, 30, fill=(255, 255, 255)))
keep("frame_polaroid.png", frames.add_frame(source, FrameType.POLAROID))
keep("frame_instax_tall.png", frames.add_frame(tall, FrameType.INSTAX_MINI))
keep("frame_instax_wide.png", frames.add_frame(source, FrameType.INSTAX_MINI))

margins = []
for frame in ("polaroid", "instax_mini"):
    for size in [(90, 60), (50, 80), (79, 79), (1, 1), (4000, 3000), (3000, 4000)]:
        f = FrameType(frame)
        margins.append({"frame": frame, "size": size, "margins": list(frames.frame_margins(f, size)),
                        "aspect": list(frames.window_aspect(f, size)), "framed": list(frames.framed_size(size, f)),
                        "margin_box": list(frames.margin_box(size, f))})

crops = []
for size, crop, frame, shape in [((400, 300), None, "polaroid", "rectangle"), ((400, 300), (10, 20, 200, 100), "instax_mini", "rectangle"),
                                 ((300, 400), None, "instax_mini", "rectangle"), ((400, 300), None, "none", "circle"),
                                 ((400, 300), (0, 0, 100, 250), "none", "circle"), ((400, 300), (5, 5, 50, 40), "none", "rounded")]:
    rect = pipeline.effective_crop(size, CropRect(*crop) if crop else None, FrameType(frame), ShapeType(shape))
    crops.append({"size": size, "crop": crop, "frame": frame, "shape": shape,
                  "result": None if rect is None else [rect.x, rect.y, rect.width, rect.height]})

flows = []
for i, (frame, shape, radius, crop, width) in enumerate([
    ("polaroid", "rounded", 20, (5, 5, 70, 50), 60),
    ("instax_mini", "circle", 10, None, None),
    ("none", "circle", 10, (10, 0, 70, 60), None),
    ("none", "rounded", 35, None, 45),
]):
    settings = EditSettings(frame=FrameType(frame), shape=ShapeType(shape), corner_radius=radius,
                            crop=CropRect(*crop) if crop else None, width=width, vignette=30)
    pipeline.apply_edits(source, settings).save(HERE / f"flow_{i}.png")
    pipeline.render_preview(source, settings, trimmed=True).save(HERE / f"flow_{i}_trimmed.png")
    pipeline.render_preview(source, settings).save(HERE / f"flow_{i}_full.png")
    flows.append({"frame": frame, "shape": shape, "radius": radius, "crop": crop, "width": width,
                  "output": list(pipeline.output_size(source.size, settings))})

(HERE / "cases.json").write_text(json.dumps({"masks": masks, "images": images, "margins": margins,
                                             "crops": crops, "flows": flows}, indent=1) + "\n")
print(len(masks), "masks", len(flows), "flows")
