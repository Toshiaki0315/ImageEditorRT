# 試作の結果（Issue #1）

Python + PyQt6 版 ImageEditor を Tauri 2（Rust + TypeScript）で作り直す前に、不安な 4 点を小さく作って確かめた。

- 計測した環境: Apple M4（10 コア）、macOS、release ビルド
- 計測用の画像: 6000×4000 の合成画像（なめらかな部分と細かい模様のある画像）を長辺 1600px に縮めたもの（1600×1067）
- 「重い設定」: Python 版のベンチマークと同じ（露出・明るさ・コントラスト・色温度・彩度・ノイズ除去・ぼかし・シャープ・ジオラマ・HDR 風・周辺減光・経年劣化・日本語の文字）

## まとめ

| 確かめること | 結果 |
|---|---|
| ① HEIC を ImageIO で読む | **できた**。EXIF の向きも直る |
| ② 設定を変えてから描き終わるまで 200ms 以内 | **できた**。重い設定でも中央値 67〜77ms・最大 81ms |
| ③ ヒラギノで日本語の文字を描く | **できた** |
| ④ EXIF（標準のタグ・ペンタックスの MakerNote・保存で MakerNote を残す） | **できた** |

Tauri で作り直してよいと判断できる。課題は「原寸の処理が Python 版より遅い」こと（下の「分かった問題」）。

## ① HEIC（macOS の ImageIO）

- `objc2-image-io` で `CGImageSource` を作り、`kCGImageSourceCreateThumbnailWithTransform` で向きを直した画像を原寸のまま取り出し、sRGB の RGBA に描いて読む（`crates/core/src/decode.rs`）
- 向き 6 の 1500×1000 の HEIC が 1000×1500 で読める（テスト `tests/decode_files.rs`）。JPEG・PNG も同じ方法で読める
- 読み込みの時間（1500×1000 の HEIC）: 初回 152ms、2 回目以降 9ms（初回は ImageIO の準備の時間）

## ② プレビューの速さ

### Rust の処理だけ（`cargo run --release -p imageeditorrt-core --example bench`）

| 処理 | Rust | Python 版 |
|---|---|---|
| プレビュー用に縮小（6000×4000 → 1600×1067、Lanczos3） | 22ms（fast_image_resize）<br>※ image クレートでは 221ms | — |
| プレビュー更新・重い設定 | 中央値 39ms（最小 38ms） | 92ms |
| プレビュー更新・露出＋彩度だけ | 1.5ms | — |
| 原寸 6000×4000・重い設定 | 829ms | 406ms（※） |
| プレビューを JPEG (q85) にする | 11ms（92KB） | — |

※ 訂正（#10）: この 406ms は旧版のベンチマーク（`scripts/bench.py`）の 4000×3000（12MP）の値で、6000×4000 ではなかった。同じ 6000×4000・同じ設定（フレーム・形なし）で旧版を測り直すと約 2.0 秒。#10 の後の作り直しは 6000×4000 で約 0.78 秒、12MP で約 0.39 秒。

### アプリでの計測（受け渡しと描画を含む）

`IMAGEEDITORRT_BENCH=1` を付けてアプリを起動すると、画面が計測して結果を出力し、終了する。
「合計」は TypeScript から `invoke` を呼んでから canvas に描き、次の画面の更新（`requestAnimationFrame`）が来るまで。各 20 回の中央値。

| 受け渡し | 設定 | 合計（最大） | 処理 | 変換 | 受け渡し | 描画 | 大きさ |
|---|---|---|---|---|---|---|---|
| JPEG | 重い | 77ms（81ms） | 53ms | 15ms | 1.3ms | 8ms | 90KB |
| JPEG | 軽い | 33ms（34ms） | 2.3ms | 9.7ms | 1.0ms | 20ms | 54KB |
| RGBA のまま | 重い | 67ms（79ms） | 62ms | 0ms | 3.9ms | 1ms | 6.7MB |
| RGBA のまま | 軽い | 33ms（34ms） | 2.2ms | 0ms | 3.1ms | 28ms | 6.7MB |

- 200ms の目標には十分に収まる
- **RGBA のまま渡しても速い**（6.7MB の受け渡しが 4ms 前後）。`tauri::ipc::Response` で生のバイト列を返すと JSON にならないため。JPEG にする時間（10〜15ms）がかからないので、本番は RGBA のままを基本にする
- 軽い設定の 33ms は、ほとんどが画面の更新の待ち（60Hz で 2 フレーム）
- アプリの中では、同じ処理がベンチマークより遅く出ることがある（39ms → 53〜62ms）。ウィンドウや WebView と CPU を分け合うためと思われる
- 注意: ウィンドウが隠れていたり、画面がスリープしていたりすると WebView の描画が止まり、計測が進まない。計測は画面を見える状態にして行う

### 速くするためにしたこと

- ガウスぼかしを「箱ぼかし 3 回」で近似（Pillow と同じ）。縦の処理を、画像を横長の帯に分けて行全体の合計をずらしていく形にし、割り算を掛け算とシフトにした（プレビュー 121ms → 39ms、原寸 2.1 秒 → 0.83 秒）
- 露出〜色温度を 1 つの変換表（LUT）にまとめて 1 回でかける
- 行・帯ごとに rayon で並列にする

## ③ ヒラギノで日本語の文字

- `/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc` の 0 番目を `ab_glyph` で読んで描ける（`crates/core/src/text.rs`）。複数行・右寄せ・不透明度に対応
- フォントの読み込みは 1 回だけ（2 回目からは読み直さない）
- テスト: 「写」「ジ」の字形があり、右下に文字が描かれる

## ④ EXIF

- **取り出し**: `kamadak-exif` で JPEG・HEIC から EXIF の TIFF の部分を取り出せる（HEIC の向きのタグも読める）
- **標準のタグ**: `kamadak-exif` で読み、Python 版と同じグループ（画像・撮影・位置情報・MakerNote・互換性・サムネイル）に分け、日本語訳の分かるタグは「ExposureTime（露出時間）」のように出す。GPS は度分秒（例: 35° 39′ 21.87″ N（35.656075））にする（`crates/core/src/exif_info.rs`）
- **MakerNote**: Python 版の `core/makernote.py`（ペンタックス・リコー・Samsung）を移した（`crates/core/src/makernote.rs`）。Python 版のテスト部品で作った 13 通りの EXIF を Python 版と Rust 版で読み、結果がすべて同じになることをテストで確かめた（`tests/makernote_matches_python.rs`）
- **保存**: Python 版の `core/tiff.py` の `ExifBlock` を移した（`crates/core/src/tiff.rs`）。EXIF を書き直しても MakerNote が元の位置に残り、JPEG に入れ直したものを `kamadak-exif` で読み直せる（`tests/exif_keep_makernote.rs`。Canon の MakerNote で確認）
- 画面の「EXIF」タブに一覧を出す

## 分かった問題・本番での課題

1. ~~**原寸の処理が Python 版の約 2 倍遅い**（829ms と 406ms）。~~ 比べた画像の大きさが違っていた（上の※）。#10 で同じ条件で比べ直し、旧版より約 2.6 倍速いことを確かめた。Pillow の C の処理（特にぼかし）が速いため。SIMD を使う、ぼかしの半径が大きいときは縮めてからぼかす、などで縮める。保存のときだけの処理なので、裏で動かせば使い心地への影響は小さい
2. **Canon・Nikon・Sony・Apple などの MakerNote の名前**: Python 版は exifread が読んでいたが、`kamadak-exif` は MakerNote を読まない。試作では「不明な形式」として番号で出している。主なメーカーのタグの表を足す必要がある
3. **画素の色の空間**: 試作は sRGB で読んでいる。Display P3 の写真（iPhone など）の色をどう扱うかを決める
4. **計測は画面が見えている必要がある**（上の注意）
5. 試作では保存・切り抜き・回転・フレーム・一括処理などは作っていない

## 動かし方

```bash
npm install
npx tauri dev                                   # 開発用に起動
npx tauri build --bundles app                   # .app を作る（target/release/bundle/macos/ImageEditorRT.app）
cargo test --workspace                          # テスト
cargo run --release -p imageeditorrt-core --example bench   # Rust の処理のベンチマーク
IMAGEEDITORRT_BENCH=1 target/release/bundle/macos/ImageEditorRT.app/Contents/MacOS/imageeditorrt   # 受け渡しを含めた計測
```
