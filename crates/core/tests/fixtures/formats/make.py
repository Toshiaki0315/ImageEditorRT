"""読み込みの変換のテスト用の画像を作る（Pillow が要る。旧版の .venv で動かす）。

    /Users/nomura/01_project/ImageEditor/.venv/bin/python crates/core/tests/fixtures/formats/make.py
"""

from pathlib import Path

from PIL import Image

HERE = Path(__file__).parent
W, H = 8, 6

# CMYK の JPEG: 左半分は白 (0,0,0,0)、右半分は赤 (0,255,255,0)
cmyk = Image.new("CMYK", (W, H), (0, 0, 0, 0))
cmyk.paste((0, 255, 255, 0), (W // 2, 0, W, H))
cmyk.save(HERE / "cmyk.jpg", quality=100)

# 16bit の PNG（グレー）: 左半分は 0、右半分は 65535
gray16 = Image.new("I;16", (W, H), 0)
gray16.paste(65535, (W // 2, 0, W, H))
gray16.save(HERE / "gray16.png")


# パレットの PNG（透過あり）: 0 番は透明、1 番は緑
palette = Image.new("P", (W, H), 0)
palette.putpalette([0, 0, 0, 0, 200, 0] + [0] * (256 - 2) * 3)
palette.paste(1, (W // 2, 0, W, H))
palette.info["transparency"] = 0
palette.save(HERE / "palette.png", transparency=0)

# アニメーション GIF（2 フレーム）: 1 枚目は青、2 枚目は黄
frames = [Image.new("RGB", (W, H), (0, 0, 255)), Image.new("RGB", (W, H), (255, 255, 0))]
frames[0].save(HERE / "animated.gif", save_all=True, append_images=frames[1:], duration=100, loop=0)

# 複数ページの TIFF（2 ページ）: 1 ページ目は赤、2 ページ目は緑
pages = [Image.new("RGB", (W, H), (255, 0, 0)), Image.new("RGB", (W, H), (0, 255, 0))]
pages[0].save(HERE / "pages.tif", save_all=True, append_images=pages[1:])

# BMP: 全体が (10, 20, 30)
Image.new("RGB", (W, H), (10, 20, 30)).save(HERE / "plain.bmp")

# 拡張子と中身が違う（中身は PNG、名前は .jpg）
Image.new("RGB", (W, H), (1, 2, 3)).save(HERE / "png_named.jpg", format="PNG")

# ImageIO は読めるが、このアプリでは扱わない形式（WebP）
Image.new("RGB", (W, H), (9, 9, 9)).save(HERE / "unsupported.webp")

# Display P3 の色の PNG（ICC プロファイル付き）: 全体が P3 の (200, 100, 50)
p3 = Path("/System/Library/ColorSync/Profiles/Display P3.icc").read_bytes()
Image.new("RGB", (W, H), (200, 100, 50)).save(HERE / "display_p3.png", icc_profile=p3)
