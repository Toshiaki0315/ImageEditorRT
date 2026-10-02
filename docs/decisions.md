# 決めたこと

作り直しの中で決めたことと、その理由。旧版と動きが違うところは「旧版との違い」に書く。

## 色の空間（#5）

- **読み込み**: macOS の ImageIO で、ファイルに埋め込まれた色のプロファイル（ICC）に従って **sRGB に変換** してから扱う。Display P3（iPhone の写真など）・Adobe RGB・CMYK・グレーの画像も、すべて sRGB の RGBA（8bit）になる。プロファイルのない画像は sRGB とみなす。
- **加工・プレビュー**: すべて sRGB の値のまま計算する（旧版の計算式も sRGB を前提にしている）。
- **保存**: sRGB の画像として書き出し、色のプロファイルは付けない（旧版と同じ。プロファイルのない画像は sRGB とみなされる）。
- **理由**: 加工の計算式を旧版とそろえやすく、どのアプリで開いても同じ色に見えるため。P3 の鮮やかな色（sRGB の外の色）は sRGB の範囲に収まる（少し鮮やかさが減る）が、写真の加工アプリとしては許容できる。
- **旧版との違い**: 旧版（Pillow）はプロファイルを使わず、P3 の値をそのまま sRGB の値として扱っていた。そのため P3 の写真は少しくすんで表示・保存されていた。作り直しでは元の色に近くなる。
- テスト: `crates/core/tests/decode_formats.rs` の `display_p3_is_converted_to_srgb`

## プレビューの受け渡し（#1）

- プレビューは RGBA の生のバイト列で渡す（`tauri::ipc::Response`）。JPEG にする時間（10〜15ms）がかからず、6.7MB でも受け渡しは 4ms 前後（`docs/prototype.md`）。

## 保存（#7）

- **形式**: 拡張子で決める（PNG・JPEG・GIF・TIFF・BMP）。HEIC を開いたときの初期の名前は `.jpg`。
- **JPEG**: jpeg-encoder で書き出す。色の間引きは旧版（Pillow）と同じ 4:2:0、品質は 1〜100（既定 90）。透過は白い背景に合成する。
- **PNG・TIFF**: 透過がなければ RGB、あれば RGBA で書く（旧版も画像のモードのまま書いていた）。
- **TIFF**: 自前で書く（圧縮なし・1 ストリップ。EXIF の項目を IFD0 に入れる）。旧版（Pillow）も既定は圧縮なし。
- **EXIF**: 試作の `ExifBlock` で整え、MakerNote を元の位置に置く。JPEG は APP1、PNG は eXIf のチャンク、TIFF は IFD0 に書く。
- **旧版との違い**: 旧版は、元の EXIF の形を `ExifBlock` で読めないときに Pillow で整えて書いていた。作り直しでは、そのときは EXIF なしで保存する（ほとんど起きない）。
- **保存の設定の保存先**: WebView の localStorage（旧版は QSettings）。

## 色の調整（#8）

- **旧版と画素まで同じにする**: 露出・明るさ・コントラスト・色温度・彩度・周辺減光・経年劣化は、旧版（Pillow）と同じ計算・丸め方にし、旧版で作った画像と画素まで一致することをテストで確かめる（`tests/adjust_matches_python.rs`）。
  - 彩度（`ImageEnhance.Color`）: Pillow の RGB → L の整数の係数、`Image.blend` の float（32bit）の計算と切り捨て。Apple Silicon の Pillow は掛け算と足し算を 1 回でまとめる（FMA）ので `mul_add` を使う
  - 周辺減光: Pillow の `radial_gradient`（256×256）を Pillow と同じバイリニアの計算（`resize::resize_gray_bilinear`）で引き伸ばし、`ImageChops.multiply`（切り捨て）で掛ける
  - 経年劣化の粒子: Python の `random.Random(seed).randbytes` と同じ乱数（メルセンヌ・ツイスタ、`pyrandom.rs`）
- 露出〜色温度は、旧版は 1 つずつかけていたが、作り直しでは 1 つの表にまとめてから 1 回でかける（整数の表どうしなので結果は同じ）。

## テイスト（#9）

- **旧版と画素まで同じにする**: 23 種すべてを旧版で作った画像と比べ、画素まで一致することを確かめる（`tests/filters_match_python.rs`）。Pillow の処理は `pillow.rs` にまとめた（`ImageEnhance` の明るさ・コントラスト・彩度、`Image.blend`、`ImageChops.screen`、`ImageOps.colorize`、`convert("L", 行列)`）。
- **ガウスぼかしを Pillow と同じにした**（`blur.rs`）: 試作は箱ぼかし 3 回の近似だったが、Pillow の「端数のある半径の箱ぼかし」（BoxBlur.c）と UnsharpMask をそのまま移した。ディテール・ジオラマもこのぼかしを使う。
- **半径は倍精度で計算してから float（32bit）にする**: 旧版は Python（倍精度）で半径を計算して Pillow に渡していたため。float32 で計算すると、まれに 1 だけ違う画素が出る。
- **JSON の名前**: テイストは旧版と同じ名前（`"high_tone"` など）。旧版のプリセットを読み込むとき（#21）にそのまま使える。

## ディテールと原寸の処理の速さ（#10）

- **ディテール**（シャープ・ぼかし・ノイズ除去）とリサイズ（Lanczos）も、旧版と画素まで同じにした（`tests/detail_matches_python.rs`・`tests/transform_matches_python.rs`）。リサイズは Pillow の計算（アルファを掛けた形で補間する）をそのまま移した（`resize::pillow_resize`）。プレビュー用の縮小だけは、速さを優先して fast_image_resize を使う（旧版も `reducing_gap` で近似していた）。
- **速さ**: 同じ 6000×4000・同じ重い設定（フレーム・形なし）で、旧版 約 2.0 秒 → 約 0.78 秒。12MP では約 0.39 秒。ぼかしは転置をやめ、真ん中の画素は端の判定なしで計算し、縦は横長の帯に分けて並列にした（結果は Pillow と同じまま）。
- 試作の報告の「旧版 0.4 秒」は 12MP の値だった（`docs/prototype.md` に訂正を書いた）。
- 内訳を測るときは `cargo run --release -p imageeditorrt-core --example profile`。

## ジオラマ（#11）

- 旧版と画素まで同じにした（`tests/diorama_matches_python.rs`）。ピントの帯のくっきり残す度合いは 1 列分を計算し、帯に沿う向きに伸ばして `Image.composite` と同じ丸めで混ぜる。
- ガイドの線（実線: くっきり残す範囲の端、破線: ぼけきる位置）の位置は Rust で計算し（`pipeline::diorama_guide`）、画面は SVG で重ねる。色は旧版と同じ（黄色に黒い影）。「トリミング実行」の表示と 100% 表示でのガイドは、それぞれ #12・#19 で足す。
