# ImageEditorRT

画像をドラッグ＆ドロップし、加工・トリミング・リサイズして保存する macOS (Apple Silicon) 向けのデスクトップアプリ。
Python + PyQt6 版の [ImageEditor](https://github.com/Toshiaki0315/ImageEditor) を、Tauri（Rust + TypeScript）で作り直したもの。

- 画像処理・EXIF: Rust
- 画面: TypeScript（Tauri の WebView）

> 作り直しの途中です。試作の結果は [docs/prototype.md](docs/prototype.md) にあります。

## 開発

```bash
npm install
npx tauri dev                    # 開発用に起動
npx tauri build --bundles app    # .app を作る
cargo test --workspace           # テスト
```

作業のルールは [CLAUDE.md](CLAUDE.md) にあります。
