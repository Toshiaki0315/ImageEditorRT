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
make dev / make app / make dmg                # 開発用に起動 / .app を作る / .dmg を作る（Makefile）
scripts/build_app.sh [--install]              # .app を作って署名・起動確認（--install で /Applications に入れる）
scripts/build_dmg.sh                          # .app からディスクイメージを作る（.app と /Applications へのリンク）
scripts/bench.sh                              # 受け渡しを含めた速さと NFR-01・02 の判定
scripts/ui_smoke.sh                           # 画面の通しの確認（アクセシビリティで開発版を操作。CI では動かさない）
scripts/make_icon.py                          # アプリのアイコン（旧版と同じ絵）を作る（ふだんは不要）
npm test                                      # 画面（TypeScript）のテスト（tests-ts/）
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
    vision.rs             # macOS の Vision の呼び出し（CPU でのやり直し・枠の変換・分けた部分ごとの認識。macOS のみ）
    faces.rs              # 顔の認識（Vision）と、隠す範囲の広げ方
    text_regions.rs       # 文字の認識（Vision。読めた文字の範囲だけ）
    horizon.rs            # 傾きの自動補正（Vision の水平線、なければ長い直線から推定）
    foreground.rs         # 被写体のマスク（Vision。macOS 14 以降）
    saliency.rs           # 目立つ部分の枠（Vision の注目度。おまかせ切り抜き）
    resize.rs             # 縮小（プレビューは fast_image_resize、保存は Pillow と画素まで同じリサイズ）
    perspective.rs        # 遠近（台形）の補正（台形を射影変換で長方形に写す。余白は出ない）
    transform.rs          # 回転・反転（8 通りの向き）・トリミング範囲の計算・リサイズの大きさ
    output.rs             # 「出力」タブのサイズ変更（欄に出す値と、編集設定に渡す幅・高さ）
    crop.rs               # トリミング範囲の編集（ドラッグ・数値の欄・比・回転と反転）の計算
    load.rs               # 画像のファイルを読む（拡張子・中身・EXIF。開く・まとめて処理・起動確認で共通）
    batch.rs              # まとめて処理（保存先の名前・画像の集め方・1 枚ずつの処理と中止）
    presets.rs            # プリセット（名前付きの加工の組み合わせ）の保存・読み込み（旧版と同じ JSON）
    pipeline.rs           # apply_edits()（保存）・render_preview()（プレビュー）。処理順はここで固定
    pipeline/             #   settings.rs（EditSettings）・sizes.rs（出力の大きさ・切り抜く範囲・換算）・guide.rs（ジオラマのガイド）・tests.rs
    prepare.rs            # 元の画像に前もってかける処理（赤目 → 肌をなめらかに → 背景。顔の枠・マスクは画面側が 1 回作って渡す）
    adjust.rs             # 変換表（LUT）・露出・明るさ・コントラスト・色温度・彩度・周辺減光・経年劣化
    local.rs              # 部分補正（円・帯の範囲の重みで、範囲の調整をかけた画像を混ぜる）
    curve.rs              # トーンカーブ（単調な 3 次補間）と色ごとの調整（8 色の色相・彩度・明るさ）
    auto.rs               # 自動補正（色の分布から露出・コントラスト・色温度を求める）
    blur.rs               # ガウスぼかし・アンシャープマスク（Pillow と画素まで同じ）
    pillow.rs             # 旧版が使っていた Pillow の処理（ImageEnhance・blend・screen など）を同じ丸め方で
    filters.rs            # テイスト（フィルター 23 種）
    lut.rs                # LUT（.cube の 3D LUT を読み、3 次元の線形補間でかける）
    effects.rs            # ディテール（シャープ・ぼかし・ノイズ除去）
    skin.rs               # 肌をなめらかに（顔のまわりの楕円だけ、輪郭を残してなめらかに）
    redeye.rs             # 赤目の補正（顔の枠の目のあたりで、強い赤だけを暗い無彩色に）
    diorama.rs            # ジオラマ風（ミニチュア風・ティルトシフト）
    frames.rs             # フレーム（ポラロイド・チェキ）
    histogram.rs          # ヒストグラム（R・G・B・輝度の分布）の計算
    shapes.rs             # 形（角丸・円）の切り抜き
    privacy.rs            # 投稿加工の範囲（ぼかし・モザイク・絵文字のスタンプ）
    color_match.rs        # 参考の写真に色を合わせる（Lab の平均と広がりを近づける。参考は色の情報だけを数値で持つ）
    background.rs         # 背景を消す（透明・色）・ぼかす・画像に置き換える（被写体のマスクで）
    collage.rs            # 並べて 1 枚に（並べ方・枠に合わせた切り抜き）
    sample.rs             # 計測用の画像
    pyrandom.rs           # Python の random.Random と同じ乱数（経年劣化の粒子を旧版とそろえる）
    text.rs               # 文字・透かし（フォント・9 か所とフレームの余白・大きさ・色・不透明度・縁取り／影。ab_glyph で描く）
    tile.rs               # 文字・ロゴを写真全体に斜めに繰り返して敷く
    logo.rs               # ロゴの透かし（画像ファイルを文字と同じ位置の決め方で重ねる）
    encode.rs             # JPEG への書き出し（プレビューの計測用）
    heic.rs               # HEIC での書き出し（ImageIO。EXIF は ImageIO のプロパティにして渡す。macOS のみ）
    save.rs               # 保存（形式・名前の決め方・元の画像への上書きの防止・EXIF を残す・著作権などを書く）
    exif_info.rs          # EXIF・GPS・MakerNote を表示用に読む
    exifread_note.rs      # 主なメーカーの MakerNote を exifread と同じに読む（表は exifread_tables.rs、自動生成）
    pyfmt.rs              # Python の値の表示（str・repr）を真似る（exifread の表示を旧版と同じにする）
    makernote.rs          # exifread も読まない MakerNote（ペンタックス・リコー・Samsung）を読む
    tiff.rs               # EXIF の IFD の読み書き（保存時に MakerNote を元の位置に置き直す）
  examples/bench.rs       # ベンチマーク
  tests/                  # ファイルを使うテスト（fixtures/ はテスト用の画像・EXIF）
src-tauri/                # Tauri のアプリ本体（コマンドで core を呼び、画面と受け渡すだけ）
  src/main.rs             # 入り口（lib.rs の run を呼ぶだけ）
  src/lib.rs              # 起動（コマンドの登録・終了の求めの扱い）だけ
  src/state.rs            # 開いている画像の状態（opened() で写しを取る・マスクと顔の枠を 1 回だけ作って覚える）と、
                          #   読み込んだ画像を状態に置くまでの共通の処理
  src/image.rs            # 開く・閉じる・プレビュー・100% 表示・クリップボードの画像・ジオラマのガイド・テイストの一覧の見本・
                          #   顔／文字／傾き／被写体の認識・自動補正・並べて 1 枚に
  src/saving.rs           # 保存（保存用のスレッドの組）と保存ダイアログの初期のパス
  src/settings.rs         # 設定パネルの選択肢と、範囲・出力の大きさの計算（計算は core）
  src/system.rs           # メニューの状態・マップで開く・終了の確認
  src/bench.rs            # 計測モード
  src/menu.rs             # メニューバー（選ばれた項目は "menu" のイベントで画面へ）
  src/open.rs             # コマンドライン引数・Finder・Dock から開く
  src/batch.rs            # まとめて処理（進み具合のイベント・中止）
  src/clipboard.rs        # クリップボード（NSPasteboard）を読む（貼り付け）・加工後の画像を書く（⇧⌘C のときだけ）
  src/edits.rs            # 写真ごとの加工を覚える（edits.json。保存・別の写真へ移る・終了のとき覚え、開いたとき「前回の加工を続ける」で当てはめる）
  src/share.rs            # 共有・印刷（加工後の画像を一時ファイルにして、macOS の共有の一覧・印刷ダイアログを出す。一時ファイルは終了時に消す）
  src/diagnostics.rs      # 想定外のエラーのログ（~/Library/Logs/ImageEditorRT/）と起動確認（--smoke-test）
  src/presets.rs          # プリセットの一覧・保存・削除・当てはめと「プリセット ▾」のメニュー
  src/recent.rs           # 最近使った項目（recent.json・「ファイル > 最近使った項目」のメニュー）
  tauri.conf.json
src/                      # 画面（TypeScript）
  main.ts                 # 入り口: 部品を作り、メニュー・イベントを機能につなぐ（計測モードも）
  bench.ts                # 計測モード（受け渡しを含めた速さ）
  app.ts                  # 共有の状態（開いている画像・保存中など）・画面の要素・部品
  editing.ts              # 設定の変更の知らせ・元に戻す／やり直す・未保存の変更の確認
  assist.ts               # 自動の処理（傾き・背景の被写体・顔／文字を隠す・肌のための顔・自動補正）の問い合わせと知らせ
  view.ts                 # 加工前との比較（左右に分けて比べるを含む）・100% 表示・左上の表示・ジオラマのガイド
  split.ts                # 左右に分けて比べる表示の計算（境目の位置・加工前の置き方。tests-ts/ で npm test）
  files.ts                # 開く・ドロップ・保存・貼り付け・まとめて処理・リセット・終了
  presetsUi.ts            # 「プリセット ▾」のメニュー・保存・当てはめ・削除
  menus.ts                # メニューの項目の ID（Rust と同じ）と使える・使えない
  status.ts               # ステータスバー・エラーの知らせ・想定外のエラー
  protocol.ts             # Rust から画素・ヒストグラムを受け取るバイト列の読み方
  keys.ts                 # キーの判定（\ キー・¥ キー）
  preview.ts              # プレビューの描画（エリアに収める・描き直しをまとめる）
  zoom.ts                 # 100% 表示（1px = 1 画素、ドラッグ・スクロールで動かす）
  batchDialog.ts          # まとめて処理のダイアログ（投稿加工の選択肢を含む）と進み具合
  paste.ts                # ⌘V で何をするか（ファイル・画像・入力欄の文字）を決める
  histogram.ts            # プレビューに重ねるヒストグラム
  history.ts              # アンドゥ／リドゥの履歴（画面の部品に依存しない。tests-ts/ で npm test）
  panel.ts / tabs.ts      # 設定パネルのスライダー・タブ
  exif.ts                 # 「EXIF」タブ（折りたたみの一覧・選んだ行のコピー・マップで開く）
  crop.ts                 # 「切り抜き」タブと、プレビュー上のドラッグでの範囲の選択（計算は core/crop.rs）
  cropOverlay.ts / cropShape.ts # 「切り抜き」の線の描画（範囲・形・ガイド）と形の輪郭のパス
  guides.ts               # 切り抜きのガイド線（三分割・黄金比・対角線）の位置の計算
  photoControls.ts        # 「切り抜き」タブの水平の補正（自動を含む）・遠近の補正と背景（消す・ぼかす・置き換える）
  perspective.ts          # 遠近の補正の向きを回転・反転に合わせて直す（tests-ts/ で npm test）
  localPanel.ts / localShapes.ts # 「加工」タブの部分補正（範囲のドラッグ）と範囲の当たり判定・線の位置
  overlay.ts              # プレビューに重ねる SVG の部品（ハンドルなど。切り抜き・投稿加工・部分補正で共通）
  storage.ts              # 画面の環境設定（localStorage）の読み書き（使えなくても既定の値で動く）
  privacy.ts / regions.ts # 「投稿加工」タブ（範囲のドラッグ・顔／文字を見つけて隠す）と範囲の計算（画面と原寸の換算もここ）
  colorPanel.ts / curve.ts # トーンカーブのグラフと色ごとの調整（曲線の計算は Rust と同じ）
  tasteGallery.ts         # テイストの一覧（見本を並べて選ぶ）
  collageDialog.ts        # 並べて 1 枚に のダイアログ
  textDialog.ts           # 「文字・透かし」のダイアログ（⌘T・「文字…」）
  textDrag.ts / textPoint.ts # 文字・ロゴの「自由」な位置をプレビューの上でドラッグする（と、割合の計算）
  lutControls.ts          # 「加工」タブの LUT（ファイルを選ぶ・外す・強さ）
  colorMatchControls.ts   # 「加工」タブの色を合わせる（参考の写真を選ぶ・外す・強さ）
  output.ts               # 「出力」タブのサイズ変更（計算は core/output.rs）
  sizes.ts                # 複数の大きさで保存（選んだ大きさの覚え方・知らせの文。tests-ts/ で npm test）
  saveOptions.ts          # 保存の設定（JPEG・HEIC の品質・EXIF・GPS・ファイルの大きさの上限。localStorage に残す）
  types.ts                # Rust とやりとりする型
  styles.css
index.html
docs/prototype.md         # 試作の結果
docs/decisions.md         # 作り直しの中で決めたこと（色の空間など。旧版との違いも書く）
```

## 設計ルール（必ず守る）

1. **画像処理と EXIF は `crates/core` に書く。** core は Tauri に依存しない。`src-tauri` は core を呼んで画面と受け渡すだけにし、TypeScript では画素を加工しない。
2. **元画像は不変。** 読み込んだ原本は保持し、プレビュー・保存のたびに原本から処理し直す。フィルターの重ね掛けをしない。
3. **処理の順番は旧版と同じにし、`pipeline.rs` の 1 か所で決める**（EXIF の回転補正 → 回転・反転 → トリミング → リサイズ → ジオラマ → フィルター → 形 → 文字 → フレーム。旧版の `docs/requirements.md` §5.1）。この版で足した処理もその流れの中に置く（水平の補正と投稿加工のぼかし・モザイクは回転・反転の直後、トーンカーブ・色ごとの調整は色の調整の中、部分補正は色の調整の直後、スタンプは経年劣化の後、ロゴ・全体に繰り返す透かしは文字と同じ）。顔の枠・マスクが要る赤目・肌・背景だけは、`prepare.rs` で回転・反転より前に元の画像にかける。トリミングの座標は、常に**回転・反転した後の原寸画像の座標**で持つ。
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
- **画面のテスト（`npm test`）は Node の型を外す読み込みで動かす。** テストで読み込むファイルには、コンストラクタの引数でのフィールドの宣言（`constructor(private readonly x: T)`）・enum など、型を外すだけでは動かない書き方を使わない。中で別のファイルを読み込むときは `import type` にする（拡張子なしの読み込みは Node では解決できない）。
- **プレビューの計測は、画面が見えていないと止まる。** `IMAGEEDITORRT_BENCH=1` を付けて起動すると計測できるが、ウィンドウが隠れていたり画面がスリープしていたりすると WebView の描画（`requestAnimationFrame`）が止まり、計測が進まない。
- **ImageIO は macOS 専用。** `decode.rs` は `#[cfg(target_os = "macos")]` で囲む。CI も macOS で動かす。
- **フォントのパスは日本語を含む。**（`/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc`）
- **release ビルドは遅い。** LTO と `codegen-units = 1` のため、`npx tauri build` は数分かかる。開発中は `npx tauri dev` や `cargo test` を使う。
- **旧版と画素まで同じにするには、Pillow の丸め方まで合わせる。** 例: `Image.blend` は float（32bit）と切り捨て、Apple Silicon では掛け算と足し算をまとめる（FMA、Rust では `mul_add`）。`ImageChops.multiply` は切り捨て。Python の `round()` は偶数への丸め（`transform::round_half_even`）。
- **画素ごとの処理は rayon で行・帯に分けて並列にする。** 1 画素ずつ `get_pixel` / `put_pixel` を呼ぶのは遅いので、生のバイト列（`as_raw` / `as_mut`）を使う。
