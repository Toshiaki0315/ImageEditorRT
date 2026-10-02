"""回転・反転・トリミング・リサイズのテスト用の期待値を、旧版の core/transform.py・core/pipeline.py で作る。

    cd /Users/nomura/01_project/ImageEditor && .venv/bin/python \
        /Users/nomura/01_project/ImageEditorRT/crates/core/tests/fixtures/transform/make.py
"""

import json
from pathlib import Path

from PIL import Image

from image_editor.core import pipeline, transform
from image_editor.core.pipeline import EditSettings
from image_editor.core.transform import AspectRatio, CropRect, Orientation, OrientOp

HERE = Path(__file__).parent


def rect(r):
    return None if r is None else [r.x, r.y, r.width, r.height]


# 元の画像: 7×5、画素ごとに違う色と透明度
source = Image.new("RGBA", (7, 5))
source.putdata([(x * 36, y * 60, (x * 7 + y * 11) % 256, 255 - (x + y) * 20) for y in range(5) for x in range(7)])
source.save(HERE / "source.png")

# 8 通りの向き
for rotation in (0, 90, 180, 270):
    for mirror in (False, True):
        Orientation(rotation, mirror).transpose(source).save(HERE / f"orient_{rotation}_{int(mirror)}.png")

cases = {}

ops = list(OrientOp)
cases["orientation_apply"] = [
    [r, m, op.value, *(lambda o: [o.rotation, o.mirror])(Orientation(r, m).apply(op))]
    for r in (0, 90, 180, 270) for m in (False, True) for op in ops
]
cases["transform_rect"] = [
    [[1, 2, 3, 2], [7, 5], op.value, rect(transform.transform_rect(CropRect(1, 2, 3, 2), (7, 5), op))] for op in ops
]
cases["clamp_crop"] = [
    [r, size, rect(transform.clamp_crop(CropRect(*r), size))]
    for r, size in [([1, 1, 3, 3], [7, 5]), ([-2, -1, 4, 4], [7, 5]), ([5, 3, 10, 10], [7, 5]), ([8, 0, 2, 2], [7, 5]),
                    ([0, 0, 0, 3], [7, 5]), ([2, 2, -1, 3], [7, 5])]
]
cases["fit_aspect"] = [
    [r, a, rect(transform.fit_aspect(CropRect(*r), tuple(a)))]
    for r, a in [([0, 0, 100, 50], [1, 1]), ([10, 20, 30, 90], [4, 3]), ([0, 0, 7, 5], [3, 2]), ([0, 0, 1, 9], [16, 9]),
                 ([5, 5, 64, 48], [62, 46])]
]
cases["constrain_rect"] = [
    [r, a, size, rect(transform.constrain_rect(CropRect(*r), tuple(a), tuple(size)))]
    for r, a, size in [([0, 0, 100, 50], [1, 1], [80, 80]), ([50, 10, 100, 100], [4, 3], [120, 80]), ([200, 0, 5, 5], [1, 1], [100, 100])]
]
cases["aspect_drag_rect"] = [
    [anchor, point, a, size, rect(transform.aspect_drag_rect(tuple(anchor), tuple(point), tuple(a), tuple(size)))]
    for anchor, point, a, size in [
        ([10, 10], [50, 20], [1, 1], [100, 100]),
        ([50, 50], [10, 40], [4, 3], [100, 100]),
        ([90, 90], [100, 0], [16, 9], [100, 100]),
        ([20, 30], [0, 0], [3, 2], [200, 100]),
        ([0, 0], [7, 3], [3, 4], [100, 100]),
    ]
]
fit_inputs = [
    ([4000, 3000], None, None, True), ([4000, 3000], 1000, None, True), ([4000, 3000], None, 1000, True),
    ([4000, 3000], 1000, 1000, True), ([4000, 3000], 1000, 500, True), ([4000, 3000], 1000, None, False),
    ([4000, 3000], 1000, 600, False), ([3, 7], 1, None, True), ([7, 3], 20000, None, True), ([333, 667], None, 100, True),
]
cases["fit_size"] = [[s, w, h, k, list(transform.fit_size(tuple(s), w, h, k))] for s, w, h, k in fit_inputs]
cases["fit_size_errors"] = [[[100, 100], 0, None], [[100, 100], None, 20001]]
cases["aspect_ratios"] = [[a.name, a.label, a.ratio(False), a.ratio(True)] for a in AspectRatio]

def settings_json(s):
    return {"rotation": s.orientation.rotation, "mirror": s.orientation.mirror, "crop": rect(s.crop),
            "width": s.width, "height": s.height, "keepAspect": s.keep_aspect}

scale_inputs = [
    EditSettings(crop=CropRect(10, 20, 300, 200), width=1000, height=None),
    EditSettings(crop=CropRect(1, 1, 1, 1), width=1, height=20000),
    EditSettings(crop=CropRect(-5, 3, 7, 0)),
]
cases["scale_settings"] = [
    [settings_json(s), f, settings_json(pipeline.scale_settings(s, f))] for s in scale_inputs for f in (0.25, 0.4, 0.5)
]
size_inputs = [
    EditSettings(),
    EditSettings(orientation=Orientation(90, False)),
    EditSettings(orientation=Orientation(270, True), crop=CropRect(100, 50, 2000, 1000)),
    EditSettings(crop=CropRect(-100, -100, 500, 500), width=100),
    EditSettings(crop=CropRect(3000, 2000, 5000, 5000), height=300, keep_aspect=False),
]
cases["output_size"] = [[settings_json(s), list(pipeline.output_size((4000, 3000), s))] for s in size_inputs]

(HERE / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1) + "\n")

# リサイズ（LANCZOS）: 40×30 のなめらかな画像（透過あり）
gradient = Image.new("RGBA", (40, 30))
gradient.putdata([(x * 6, y * 8, 128 + (x - y) * 2, 255 if x < 30 else 128) for y in range(30) for x in range(40)])
gradient.save(HERE / "gradient.png")
for size in [(17, 13), (80, 61), (40, 9)]:
    transform.resize(gradient, size).save(HERE / f"resize_{size[0]}x{size[1]}.png")

# 処理の流れ（回転・反転 → トリミング → リサイズ）。加工はかけない
flow = EditSettings(orientation=Orientation(90, True), crop=CropRect(3, 5, 20, 24), width=15)
pipeline.apply_edits(gradient, flow).save(HERE / "apply_edits.png")
(HERE / "apply_edits.json").write_text(json.dumps(settings_json(flow)) + "\n")
print("ok")
