# CLAUDE.md

このファイルは Claude Code がこのリポジトリで作業するときの前提・ルールです。

ImageEditorRT は、Python + PyQt6 版の [ImageEditor](https://github.com/Toshiaki0315/ImageEditor) を Tauri（Rust + TypeScript）で作り直したものです。
- **仕様は旧版に合わせる**：旧版の `docs/requirements.md` と旧版の動きを正とします。手元では `/Users/nomura/01_project/ImageEditor` にあります。
- **加工の結果は旧版とそろえる**：計算式は旧版の `src/image_editor/core/` から移します。
- **試作の結果**：`docs/prototype.md` にあります。

## プロジェクト概要

画像をドラッグ＆ドロップし、加工・トリミング・リサイズして保存する macOS (Apple Silicon) 向けのデスクトップアプリ。

- アプリ: Tauri 2（アプリ名 ImageEditorRT、識別子 `io.github.toshiaki0315.imageeditorrt`）
- 画像処理・EXIF: Rust（`crates/core`。Tauri に依存しない）
- 画面: TypeScript + Vite（フレームワークなし）
- 画像の読み込み: macOS の ImageIO（HEIC・JPEG・PNG など。EXIF の向きもここで直す）
- EXIF: kamadak-exif で読み、MakerNote は主なメーカー（Canon・Nikon・Sony・Apple など、旧版の exifread が読んでいたもの）を `exifread_note.rs`、ペンタックス・リコー・Samsung などを `makernote.rs` で読む。保存は `tiff.rs` の `ExifBlock`
- テスト: `cargo test`（Rust）、`tsc`（TypeScript の型チェック）
- Lint/Format: `cargo fmt`（設定は `rustfmt.toml`）・`cargo clippy`
- Rust のバージョン: `rust-toolchain.toml` で固定する（手元と CI で clippy の指摘をそろえるため）。上げるときは CI の `dtolnay/rust-toolchain@<版>` も同じにする
- 構成管理: GitHub（Issue → ブランチ → PR）

## コマンド

```bash
npm install                                   # 依存インストール（初回・package.json を変えたとき）
npx tauri dev                                 # 開発用に起動
npx tauri build --bundles app                 # .app を作る（target/release/bundle/macos/ImageEditorRT.app）
cargo test --workspace                        # Rust のテスト
cargo fmt --all                               # 整形（確認だけなら --check）
cargo clippy --workspace --all-targets -- -D warnings   # Lint
npm run build                                 # TypeScript の型チェックと画面のビルド
cargo run --release -p imageeditorrt-core --example bench   # 処理の速さのベンチマーク
cargo run --release -p imageeditorrt-core --example profile # 原寸の処理の内訳
```

## ディレクトリ構成

```
crates/core/              # ★ Tauri に依存しない画像処理・EXIF（imageeditorrt_core）
  src/
    formats.rs            # 読み込める形式・拡張子・透過の有無
    decode.rs             # ImageIO での読み込み（向きを直した sRGB の RGBA にする。macOS のみ）
    resize.rs             # 縮小（プレビューは fast_image_resize、保存は Pillow と画素まで同じリサイズ）
    transform.rs          # 回転・反転（8 通りの向き）・トリミング範囲の計算・リサイズの大きさ
    output.rs             # 「出力」タブのサイズ変更（欄に出す値と、編集設定に渡す幅・高さ）
    crop.rs               # トリミング範囲の編集（ドラッグ・数値の欄・比・回転と反転）の計算
    pipeline.rs           # EditSettings と apply_edits()（保存）・render_preview()（プレビュー）。処理順はここで固定
    adjust.rs             # 変換表（LUT）・露出・明るさ・コントラスト・色温度・彩度・周辺減光・経年劣化
    blur.rs               # ガウスぼかし・アンシャープマスク（Pillow と画素まで同じ）
    pillow.rs             # 旧版が使っていた Pillow の処理（ImageEnhance・blend・screen など）を同じ丸め方で
    filters.rs            # テイスト（フィルター 23 種）
    effects.rs            # ディテール（シャープ・ぼかし・ノイズ除去）
    diorama.rs            # ジオラマ風（ミニチュア風・ティルトシフト）
    frames.rs             # フレーム（ポラロイド・チェキ）
    histogram.rs          # ヒストグラム（R・G・B・輝度の分布）の計算
    shapes.rs             # 形（角丸・円）の切り抜き
    sample.rs             # 計測用の画像
    pyrandom.rs           # Python の random.Random と同じ乱数（経年劣化の粒子を旧版とそろえる）
    text.rs               # 文字・透かし（フォント・9 か所とフレームの余白・大きさ・色・不透明度。ab_glyph で描く）
    encode.rs             # JPEG への書き出し（プレビューの計測用）
    save.rs               # 保存（形式・名前の決め方・元の画像への上書きの防止・EXIF を残す）
    exif_info.rs          # EXIF・GPS・MakerNote を表示用に読む
    exifread_note.rs      # 主なメーカーの MakerNote を exifread と同じに読む（表は exifread_tables.rs、自動生成）
    makernote.rs          # exifread も読まない MakerNote（ペンタックス・リコー・Samsung）を読む
    tiff.rs               # EXIF の IFD の読み書き（保存時に MakerNote を元の位置に置き直す）
  examples/bench.rs       # ベンチマーク
  tests/                  # ファイルを使うテスト（fixtures/ はテスト用の画像・EXIF）
src-tauri/                # Tauri のアプリ本体（コマンドで core を呼び、画面と受け渡すだけ）
  src/lib.rs              # コマンド（開く・プレビュー）と起動
  src/menu.rs             # メニューバー（選ばれた項目は "menu" のイベントで画面へ）
  src/open.rs             # コマンドライン引数・Finder・Dock から開く
  tauri.conf.json
src/                      # 画面（TypeScript）
  main.ts                 # 起動・開く・ドロップ・ステータスバー
  preview.ts              # プレビューの描画（エリアに収める・描き直しをまとめる）
  panel.ts / tabs.ts      # 設定パネルのスライダー・タブ
  exif.ts                 # 「EXIF」タブ（折りたたみの一覧・選んだ行のコピー・マップで開く）
  crop.ts                 # 「切り抜き」タブと、プレビュー上のドラッグでの範囲の選択（計算は core/crop.rs）
  textDialog.ts           # 「文字・透かし」のダイアログ（⌘T・「文字…」）
  output.ts               # 「出力」タブのサイズ変更（計算は core/output.rs）
  saveOptions.ts          # 「出力」タブの保存の設定（JPEG 品質・EXIF・GPS。localStorage に残す）
  types.ts                # Rust とやりとりする型
  styles.css
index.html
docs/prototype.md         # 試作の結果
docs/decisions.md         # 作り直しの中で決めたこと（色の空間など。旧版との違いも書く）
```

## 設計ルール（必ず守る）

1. **画像処理と EXIF は `crates/core` に書く。** core は Tauri に依存しない。`src-tauri` は core を呼んで画面と受け渡すだけにし、TypeScript では画素を加工しない。
2. **元画像は不変。** 読み込んだ原本は保持し、プレビュー・保存のたびに原本から処理し直す。フィルターの重ね掛けをしない。
3. **処理の順番は旧版と同じにし、`pipeline.rs` の 1 か所で決める**（EXIF の回転補正 → 回転・反転 → トリミング → リサイズ → ジオラマ → フィルター → 形 → 文字 → フレーム。旧版の `docs/requirements.md` §5.1）。トリミングの座標は、常に**回転・反転した後の原寸画像の座標**で持つ。
4. **プレビューは縮小版で処理する。** 長辺 1600px に縮めた画像に設定をかけて表示し、保存のときだけ原寸で処理する。
5. **プレビューの受け渡しは生のバイト列で行う。** `tauri::ipc::Response` で返し、JSON や base64 にしない（`docs/prototype.md`）。設定を変えてから描き終わるまで 200ms 以内を保つ。
6. **重い処理で画面を止めない。** 重い処理を行うコマンドは `async fn` にし、原寸の処理・保存はメインスレッドで行わない。
7. **加工の結果は旧版とそろえる。** 計算式を移したら、旧版と同じ入力で同じ（またはほぼ同じ）結果になることをテストで確かめる。期待値は推測で書かず、旧版で計算して決める。
8. **公開する関数・型には、日本語で短いドキュメントコメント（`///`）を書く。**
9. **依存するクレート・npm パッケージを増やすときは、PR の説明に理由を書く。** `image` クレートは `default-features = false` で、必要な機能だけを有効にする。

## Git / GitHub ワークフロー

作業は GitHub Issue 単位で行う。`gh` CLI が使える前提。

1. `gh issue view <番号>` で受け入れ条件を確認する。
2. `main` を最新にしてからブランチを切る。
   ```bash
   git switch main && git pull && git switch -c feat/<番号>-<短い英語名>
   ```
   ブランチの種別は `feat/` `fix/` `refactor/` `docs/` `test/` `chore/` から選ぶ。
3. 実装してテストを足し、次がすべて通ることを確かめる。
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `npm run build`
4. コミットは Conventional Commits の形にする。本文は日本語でよい。例: `feat(core): セピアフィルターを追加 (#5)`
5. `git push -u origin HEAD` の後、`gh pr create` で PR を作る。PR の本文には `Closes #<番号>` と確認の手順を書く。
6. **マージはユーザーの指示があるときだけ行う。**
   - ユーザーの指示があれば、CI が緑であることを確かめてから `gh pr merge --squash --delete-branch` で squash マージしてよい。
   - 指示がなければマージしない。
   - `main` へ直接 push しない。`git push --force` もしない。

## 完了の定義 (Definition of Done)

- Issue の受け入れ条件をすべて満たす
- `crates/core` の変更にはテストがある
- fmt・clippy・テスト・`npm run build` が通る（CI も緑）
- 画面の変更は、PR に手動での確認の手順を書く（できればスクリーンショットも付ける）

## 注意点・既知の落とし穴

- **EXIF を書き直すと MakerNote が壊れることがある。** MakerNote の中の値の位置がずれるため。保存では必ず `tiff::ExifBlock`（MakerNote を元の位置に置き直す）を通す。
- **JPEG は透過を持てない。** 透過のある画像を JPEG で保存するときは、白い背景に合成する。
- **プレビューの計測は、画面が見えていないと止まる。** `IMAGEEDITORRT_BENCH=1` を付けて起動すると計測できるが、ウィンドウが隠れていたり画面がスリープしていたりすると WebView の描画（`requestAnimationFrame`）が止まり、計測が進まない。
- **ImageIO は macOS 専用。** `decode.rs` は `#[cfg(target_os = "macos")]` で囲む。CI も macOS で動かす。
- **フォントのパスは日本語を含む。**（`/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc`）
- **release ビルドは遅い。** LTO と `codegen-units = 1` のため、`npx tauri build` は数分かかる。開発中は `npx tauri dev` や `cargo test` を使う。
- **旧版と画素まで同じにするには、Pillow の丸め方まで合わせる。** 例: `Image.blend` は float（32bit）と切り捨て、Apple Silicon では掛け算と足し算をまとめる（FMA、Rust では `mul_add`）。`ImageChops.multiply` は切り捨て。Python の `round()` は偶数への丸め（`transform::round_half_even`）。
- **画素ごとの処理は rayon で行・帯に分けて並列にする。** 1 画素ずつ `get_pixel` / `put_pixel` を呼ぶのは遅いので、生のバイト列（`as_raw` / `as_mut`）を使う。
