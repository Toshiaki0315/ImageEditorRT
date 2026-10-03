# ImageEditorRT

画像をドラッグ＆ドロップし、加工・トリミング・リサイズして保存する macOS (Apple Silicon) 向けのデスクトップアプリ。
Python + PyQt6 版の [ImageEditor](https://github.com/Toshiaki0315/ImageEditor) を、Tauri（Rust + TypeScript）で作り直したもの。
加工の結果は旧版と画素までそろえてある（文字の描き方だけは少し違う。[docs/decisions.md](docs/decisions.md)）。

- 画像処理・EXIF: Rust
- 画面: TypeScript（Tauri の WebView）

## インストール

自分の Mac（Apple Silicon、macOS 13 以降）で使う。署名は ad-hoc（他の Mac への配布はしない）。

```bash
scripts/build_app.sh --install   # ビルドして /Applications/ImageEditorRT.app に入れる（Node.js と Rust が必要）
```

よく使うコマンドは `make` でも動かせる:

| コマンド | すること |
|---|---|
| `make dev` | 開発用に起動する（画面の変更はすぐ反映） |
| `make app` | `.app` を作る（署名・起動確認まで。`target/release/bundle/macos/ImageEditorRT.app`） |
| `make dmg` | `.app` を作ってからディスクイメージにする（`target/release/bundle/dmg/ImageEditorRT_<版>_arm64.dmg`。開いて Applications にドラッグすればインストールできる） |

どれも、画面の依存パッケージ（`node_modules`）がなければ先に入れる。

- ビルドの最後に、画面を出さずに起動確認をする（作った画像と HEIC の画像を読めるか）。
- ImageEditorRT が起動していると入れ替えない。終了してから、もう一度実行する。
- `--install` を付けなければ、`target/release/bundle/macos/ImageEditorRT.app` を作るだけ。
- Finder の「このアプリケーションで開く」・Dock のアイコンへのドロップでも画像を開ける（既定のアプリは変えない）。

## 使い方

| やりたいこと | 操作 |
|---|---|
| 画像を開く | ウィンドウにドロップ・⌘O・⌘V（クリップボードの画像・Finder でコピーしたファイル） |
| 加工する | 右の「加工」タブ（テイスト・露出〜経年劣化・ディテール）。スライダーはダブルクリックで既定値 |
| 切り抜く | 「切り抜き」タブで比を選び、プレビューをドラッグ。回転・反転・フレーム・形（角丸・円）もここ |
| 大きさを変える | 「出力」タブで幅・高さ。保存の設定（JPEG 品質・EXIF・位置情報）も |
| ジオラマ風 | 「ジオラマ」タブ。ピントの帯のガイドがプレビューに出る |
| 文字・透かし | ⌘T か「文字…」 |
| 見比べる・確かめる | `\` キー・「加工前」ボタンを押している間は加工前。⌘1 で 100% 表示（⌘0 で戻る）、⇧⌘H でヒストグラム |
| 戻す | ⌘Z・⇧⌘Z。「リセット」で画像を閉じる |
| 加工を使い回す | 「プリセット ▾」で保存・呼び出し。「ファイル > まとめて処理…」で複数の画像に同じ加工 |
| 保存する | ⌘S（元の画像には上書きしない） |
| 撮影情報を見る | 「EXIF」タブ（MakerNote も。位置情報はマップで開ける） |

- プリセットは `~/Library/Application Support/ImageEditorRT/presets.json` に保存する（まだなければ旧版のプリセットを読む）。
- 想定外のエラーはダイアログで知らせ、`~/Library/Logs/ImageEditorRT/imageeditorrt.log` に書く。

## 速さの確認

```bash
scripts/bench.sh    # 測っている間はウィンドウを前に出しておく
```

12MP の JPEG の読み込みからプレビュー表示まで・重い設定のプレビュー更新・保存中の更新を測り、最後に旧版の
NFR-01（読み込み→表示 1 秒以内）・NFR-02（プレビュー更新 200ms 以内）の判定を出す。

## 開発

```bash
make dev                         # 開発用に起動（npx tauri dev）
cargo test --workspace           # Rust のテスト
npm test                         # 画面（TypeScript）のテスト
npm run build                    # TypeScript の型チェックと画面のビルド
```

作業のルールは [CLAUDE.md](CLAUDE.md)、決めたことは [docs/decisions.md](docs/decisions.md)、試作の結果は
[docs/prototype.md](docs/prototype.md) にあります。アイコンは `scripts/make_icon.py`（旧版と同じ絵）で作った。
