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
