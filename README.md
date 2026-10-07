# ImageEditorRT

画像をドラッグ＆ドロップし、加工・トリミング・リサイズして保存する macOS (Apple Silicon) 向けのデスクトップアプリ。
Python + PyQt6 版の [ImageEditor](https://github.com/Toshiaki0315/ImageEditor) を、Tauri（Rust + TypeScript）で作り直したもの。
加工の結果は旧版と画素までそろえてある（文字の描き方だけは少し違う。[docs/decisions.md](docs/decisions.md)）。

主な機能:

- テイスト 23 種（強さ 0〜200%）・露出〜経年劣化の色の調整（ハイライト／シャドウを含む）・シャープ／ぼかし／ノイズ除去・ジオラマ風
- 回転・反転・水平の補正（傾きの自動補正つき）・トリミング（比の固定）・リサイズ・フレーム（ポラロイド・チェキ）・形（角丸・円）・文字／透かし
- 投稿加工（位置情報の削除・範囲のぼかし／モザイク・絵文字のスタンプ・顔を自動で見つけて隠す）
- 加工前との比較・100% 表示・ヒストグラム・元に戻す／やり直す・プリセット・まとめて処理
- EXIF の表示（GPS・主なメーカーの MakerNote）と保存（EXIF・位置情報を残すかを選べる）
- PNG / JPEG / GIF / TIFF / BMP を読み書き、HEIC / HEIF は読み込みのみ

## 実行環境

| 項目 | 内容 |
|---|---|
| OS | macOS 13 (Ventura) 以降 |
| CPU | Apple Silicon（M1 以降） |
| 確認した環境 | macOS 27（Apple M4） |
| 配布 | 自分の Mac で使う前提。署名は ad-hoc（Developer ID の署名・公証はしていない） |

画像の読み込みには macOS の ImageIO、文字にはヒラギノなど macOS に入っているフォントを使うので、ほかの OS では動かない。

## 利用アーキテクチャ

```
┌─────────── 画面（TypeScript + Vite、Tauri の WebView） ───────────┐
│ src/main.ts（入り口）・app（状態）・editing・view・files・presetsUi … │
└──────────────┬─────────────────────────────────────────────────────┘
               │ invoke（設定は JSON、画素・ヒストグラムはバイト列のまま）
┌──────────────▼──────────── アプリ本体（src-tauri、Rust） ──────────┐
│ コマンド（開く・プレビュー・保存・プリセット・まとめて処理・メニュー）│
│ 重い処理は別のスレッド（spawn_blocking・保存用の rayon の組）         │
└──────────────┬─────────────────────────────────────────────────────┘
               │ 関数を呼ぶだけ
┌──────────────▼──────────── 画像処理・EXIF（crates/core、Rust） ────┐
│ Tauri に依存しない処理。旧版と画素まで同じかをテストで確かめる       │
└────────────────────────────────────────────────────────────────────┘
```

| 部分 | 使っているもの |
|---|---|
| アプリ | [Tauri](https://tauri.app/) 2.12・tauri-plugin-dialog |
| 画像処理 | Rust 1.99（`rust-toolchain.toml` で固定）・image 0.25・rayon（並列）・fast_image_resize（プレビューの縮小）・jpeg-encoder |
| 画像の読み込み | macOS の ImageIO（objc2-image-io。HEIC と EXIF の向きの補正もここ） |
| 顔・水平線の認識 | macOS の Vision（objc2-vision。投稿加工・傾きの自動補正） |
| EXIF | kamadak-exif（標準のタグ）・自前の読み取り（MakerNote は旧版の exifread と同じ読み方） |
| 文字 | ab_glyph（macOS のフォント） |
| 画面 | TypeScript 6・Vite 8（フレームワークなし） |
| テスト | `cargo test`（Rust）・Node の組み込みのテスト（画面のロジック）・アクセシビリティで操作する通しの確認 |

- 処理の順番は旧版と同じ: 向きの補正 → 回転・反転 →（水平の補正）→ トリミング → リサイズ → ジオラマ → テイスト → 形 → 文字 → フレーム。
- プレビューは長辺 1600px に縮めた画像に設定をかけ、保存のときだけ原寸で処理する。元の画像は変えない。
- ファイルの構成と作業のルールは [CLAUDE.md](CLAUDE.md)、決めたことは [docs/decisions.md](docs/decisions.md) にある。

## 開発環境の構築方法

1. Xcode のコマンドラインツールを入れる: `xcode-select --install`
2. Rust を入れる（[rustup](https://rustup.rs/)）。版は `rust-toolchain.toml`（1.99）で決まっていて、初めてビルドしたときに自動で入る
3. Node.js 22.6 以降を入れる（画面のテストで、TypeScript を型を外して直接動かすため）
4. リポジトリを取ってきて、画面の依存パッケージを入れる:

```bash
git clone https://github.com/Toshiaki0315/ImageEditorRT.git
cd ImageEditorRT
npm install
```

5. 開発用に起動する（画面の変更はすぐ反映される）:

```bash
make dev
```

テスト・確認:

```bash
cargo test --workspace     # Rust のテスト（旧版と画素まで同じかの比較を含む）
npm test                   # 画面のロジックのテスト
npm run build              # TypeScript の型チェックと画面のビルド
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings   # 整形と Lint
```

旧版と比べるテストの期待値（`crates/core/tests/fixtures/*/make.py`）を作り直すときは、旧版（ImageEditor）の
仮想環境の Python で動かす（ふだんの開発では不要）。アイコンは `scripts/make_icon.py`（旧版と同じ絵）で作った。

## ビルド方法

| コマンド | すること | できるもの |
|---|---|---|
| `make app` | `.app` を作り、ad-hoc で署名し、起動確認をする（画面を出さずに PNG と HEIC を読めるか） | `target/release/bundle/macos/ImageEditorRT.app` |
| `make dmg` | `.app` を作ってからディスクイメージにする（開くと、ほかのアプリと同じく大きなアイコンで `.app` と Applications へのリンクが並ぶ） | `target/release/bundle/dmg/ImageEditorRT_<版>_arm64.dmg` |
| `scripts/build_app.sh --install` | `make app` と同じことをしてから `/Applications` に入れ、「このアプリケーションで開く」に出るよう登録する | `/Applications/ImageEditorRT.app` |

- インストールのとき ImageEditorRT が起動していると、入れ替えずに止まる。終了してから、もう一度実行する。
- dmg を開いて `.app` を Applications にドラッグしてもインストールできる。
- `make dmg` は dmg の見た目を Finder に設定させるので、作っている間に Finder のウィンドウが一瞬開く（初回は「Finder を操作する」許可を求められる）。同じ名前の dmg（ImageEditorRT）を開いたままだと作れないので、取り出してから実行する。
- どれも、画面の依存パッケージ（`node_modules`）がなければ先に入れる。

## 利用方法

| やりたいこと | 操作 |
|---|---|
| 画像を開く | ウィンドウにドロップ・⌘O・⌘V（クリップボードの画像・Finder でコピーしたファイル）・Finder の「このアプリケーションで開く」 |
| 加工する | 右の「加工」タブ（テイストと強さ・露出〜経年劣化・ディテール）。「一覧…」で今の写真にかけた見本を見比べてテイストを選べる。スライダーはダブルクリックで既定値 |
| 切り抜く | 「切り抜き」タブで比を選び（SNS の縦長は「縦向き」で 4:5・9:16）、プレビューをドラッグ。回転・反転・水平の補正（±45°。「傾きを自動で直す」も）・フレーム・形（角丸・円）もここ |
| 大きさを変える | 「出力」タブで幅・高さ。保存の設定（JPEG 品質・EXIF・位置情報）も |
| ジオラマ風 | 「ジオラマ」タブ。ピントの帯のガイドがプレビューに出る |
| 投稿前に隠す | 「投稿加工」タブ。位置情報を消して保存・ぼかし／モザイク／スタンプ（絵文字）を選んでプレビューをドラッグ（範囲ごとに強さ・絵文字を変えられる）。「顔を見つけて隠す」で写っている顔をまとめてぼかし・モザイク・スタンプで隠せる |
| 文字・透かし | ⌘T か「文字…」 |
| 見比べる・確かめる | `\` キー・「加工前」ボタンを押している間は加工前。⌘1 で 100% 表示（⌘0 で戻る）、⇧⌘H でヒストグラム |
| 戻す | ⌘Z・⇧⌘Z。「リセット」で画像を閉じる |
| 加工を使い回す | 「プリセット ▾」で保存・呼び出し・削除。「ファイル > まとめて処理…」で複数の画像に同じ加工 |
| 保存する | ⌘S（元の画像には上書きしない）。保存していない変更があるまま終了・別の画像を開く・リセットすると確認が出る |
| 撮影情報を見る | 「EXIF」タブ（MakerNote も。位置情報はマップで開ける） |

## その他・備考

- **旧版との違い**: 文字の形は Pillow（FreeType）ではなく ab_glyph で描くので、縁や位置が 1px 程度違うことがある。テイストの強さ・水平の補正・ハイライト／シャドウ・テイストの一覧・投稿加工・未保存のまま終了するときの確認は、この版で足した機能。
- **保存される場所**:
  - プリセット: `~/Library/Application Support/ImageEditorRT/presets.json`（まだなければ旧版の `~/Library/Application Support/ImageEditor/presets.json` を読む。旧版のファイルは書き換えない）
  - 想定外のエラーのログ: `~/Library/Logs/ImageEditorRT/imageeditorrt.log`
  - 画面の環境設定（最後に開いたタブ・ヒストグラムの表示・保存の設定など）: アプリの WebView の中
- **署名**: ad-hoc の署名なので、ほかの Mac にコピーすると Gatekeeper に止められる。
- **速さの確認**: `scripts/bench.sh`。12MP の JPEG の読み込み〜表示・重い設定のプレビュー更新などを測り、旧版の目標（読み込み〜表示 1 秒以内・プレビュー更新 200ms 以内）の判定を出す。測っている間はウィンドウを前に出しておく。Apple M4 では読み込み〜表示 84ms・重い設定の更新 29ms。
- **画面の通しの確認**: `scripts/ui_smoke.sh`。開発版を起動し、macOS のアクセシビリティで開く・スライダー・元に戻す・水平の補正・テイストの一覧・100% 表示・リセット・終了を確かめる（キー入力は送らず、クリップボードも触らない）。動かすターミナルに「システム設定 > プライバシーとセキュリティ > アクセシビリティ」の許可が必要。
- **CI**: GitHub Actions（macOS 14）で、整形・Lint・Rust と画面のテスト・`.app` のビルドと起動確認をする。
- **作り直しの経緯**: 試作の結果は [docs/prototype.md](docs/prototype.md)、Issue ごとに決めたことは [docs/decisions.md](docs/decisions.md)。

## ライセンス

[MIT License](LICENSE)（Copyright (c) 2026 Toshiaki Nomura）。

MakerNote のタグの表と読み方（exifread 由来、BSD-3-Clause）・Pillow の計算を移した部分（MIT-CMU License）・
Mersenne Twister（BSD-3-Clause）は、それぞれのライセンスに従う。著作権表示と条件は
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) にある。依存パッケージのライセンスは、それぞれのパッケージに含まれている。
