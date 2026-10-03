# テスト用の画像

どれも合成した画像（写真ではない）。EXIF・MakerNote は Python 版 ImageEditor の
`tests/exif_samples.py` で組み立てたもの。

| ファイル | 中身 |
|---|---|
| `rotated.heic` | 1500×1000 の HEIC。EXIF の向き 6（時計回りに 90° 回して見る） |
| `rotated.jpg` | 64×48 の JPEG。EXIF の向き 6 |
| `pentax.jpg` | 64×48 の JPEG。EXIF（撮影日時・露出時間）・GPS・ペンタックスの MakerNote（"PENTAX \0" 形式、位置は MakerNote の先頭が基準） |
| `canon.jpg` | 64×48 の JPEG。Canon の MakerNote（位置は TIFF の先頭が基準。保存で位置がずれると壊れる） |
| `makernote/*.tiff` | MakerNote の読み取りのテスト用の EXIF（TIFF の部分だけ）。ペンタックス・リコー・Samsung・不明な形式・壊れたものなど 13 通り |
| `makernote/expected.json` | 上の EXIF を Python 版の `core/makernote.py` で読んだ結果（Rust 版と同じになるかを比べる） |
| `formats/*` | 読み込みの変換のテスト用（CMYK の JPEG・16bit の PNG・パレットの PNG・アニメーション GIF・複数ページの TIFF・BMP・中身と拡張子が違うもの・WebP・Display P3）。`formats/make.py`（Pillow）で作ったもの |
| `transform/*` | 回転・反転・トリミング・リサイズの期待値。旧版の `core/transform.py`・`core/pipeline.py` で作ったもの（`transform/make.py`）。`cases.json` は範囲・大きさの計算、PNG は画像 |
| `adjust/*` | 色の調整（露出〜経年劣化）の期待値。旧版の `core/effects.py`・`core/pipeline.py` で作ったもの（`adjust/make.py`）。画素まで一致することを確かめる |
| `filters/*` | テイスト（フィルター 23 種）の期待値。旧版の `core/filters.py` で作ったもの（`filters/make.py`）。画素まで一致することを確かめる |
| `detail/*` | ディテール（シャープ・ぼかし・ノイズ除去）と、保存・縮小プレビューの流れの期待値。旧版の `core/effects.py`・`core/pipeline.py` で作ったもの（`detail/make.py`）。画素まで一致することを確かめる |
| `diorama/*` | ジオラマ風の期待値（横・縦の帯、写真の範囲あり・なし、保存とプレビューの流れ）。旧版の `core/diorama.py`・`core/pipeline.py` で作ったもの（`diorama/make.py`）。画素まで一致することを確かめる |
| `frame/*` | フレーム・形の期待値（マスク・切り抜き・余白・実際に切り抜く範囲・保存とプレビューの流れ）。旧版の `core/frames.py`・`core/shapes.py`・`core/pipeline.py` で作ったもの（`frame/make.py`）。画素まで一致することを確かめる |
| `makernote_makers/*.tiff` | 主なメーカー（Canon・Nikon・Sony・Apple・Fujifilm・Olympus・Casio・DJI）の MakerNote を入れた EXIF（TIFF の部分だけ）。19 通り。`makernote_makers/make.py` で旧版のテスト部品を使って作ったもの |
| `makernote_makers/expected.json` | 上の EXIF を旧版の `core/exif_info.py`（exifread）で読んだ結果（MakerNote の項目・形式の名前・UserComment の有無） |
| `histogram/cases.json` | ヒストグラムの期待値（切り抜き・形・フレーム・文字・トリミング実行の表示の組み合わせ 7 通り）。旧版の `core/pipeline.py` の `render_preview_with_histogram` で数えたもの（`histogram/make.py`。画像は `frame/` のもの） |
| `presets/*.json` | プリセットの読み書きのテスト用。`saved.json` は旧版の `save_presets` で書いたファイル、ほかは壊れた項目・壊れたファイル。`expected.json` は旧版の `load_presets` で読んだ結果（`presets/make.py`） |
