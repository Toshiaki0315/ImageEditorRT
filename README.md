# ImageEditorRT

画像をドラッグ＆ドロップし、加工・トリミング・リサイズして保存する macOS (Apple Silicon) 向けのデスクトップアプリ。
Python + PyQt6 版の [ImageEditor](https://github.com/Toshiaki0315/ImageEditor) を、Tauri（Rust + TypeScript）で作り直したもの。

- 画像処理・EXIF: Rust
- 画面: TypeScript（Tauri の WebView）

> 作り直しの途中です。まず試作で、HEIC の読み込み・プレビューの速さ・日本語の文字・EXIF を確かめています。
