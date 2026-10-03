#!/usr/bin/env bash
# 速さを測る（受け渡しを含めたプレビューの更新・12MP の読み込み・保存中の更新）。
# 最後に旧版の NFR-01（読み込み→表示 1 秒以内）・NFR-02（重い設定の更新 200ms 以内）の判定を出す。
#
# 使い方:
#   scripts/bench.sh            # ビルド済みの .app で測る（なければ scripts/build_app.sh でビルド）
#
# 測っている間はウィンドウを前に出しておく（隠れていると描画が止まり、計測が進まない）。
set -euo pipefail

cd "$(dirname "$0")/.."

BIN="target/release/bundle/macos/ImageEditorRT.app/Contents/MacOS/imageeditorrt"
if [[ ! -x "$BIN" ]]; then
    scripts/build_app.sh
fi

# 測っている間は画面をスリープさせない
IMAGEEDITORRT_BENCH=1 caffeinate -d "$BIN"
