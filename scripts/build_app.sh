#!/usr/bin/env bash
# ImageEditorRT.app をビルドする（自分の Mac〔Apple Silicon〕で使う。署名は ad-hoc）。
#
# 使い方:
#   scripts/build_app.sh             # ビルドして起動確認まで
#   scripts/build_app.sh --install   # さらに /Applications にインストールする
#
# インストールするとき、ImageEditorRT が起動していたら入れ替えずに止まる（終了してから、もう一度実行する）。
set -euo pipefail

cd "$(dirname "$0")/.."

APP="target/release/bundle/macos/ImageEditorRT.app"
BIN="$APP/Contents/MacOS/imageeditorrt"
INSTALL_TO="/Applications/ImageEditorRT.app"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

install=false
for arg in "$@"; do
    case "$arg" in
        --install) install=true ;;
        *) echo "知らない引数です: $arg（使えるのは --install だけ）" >&2; exit 2 ;;
    esac
done

# 起動中のアプリは入れ替えない（作業中の画像が失われないよう、自分で終了してもらう）
check_not_running() {
    if pgrep -f "^$INSTALL_TO/Contents/MacOS/" >/dev/null; then
        echo "ImageEditorRT が起動しています。終了してから、もう一度実行してください。" >&2
        exit 1
    fi
}
if $install; then
    check_not_running
fi

if [[ ! -d node_modules ]]; then
    echo "==> 画面の依存パッケージをインストール"
    npm ci
fi

echo "==> .app をビルド"
npx tauri build --bundles app

echo "==> 署名（ad-hoc）"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

echo "==> 起動確認"
"$BIN" --smoke-test

echo "==> HEIC を開けるかの確認"
SAMPLE_DIR="$(mktemp -d)"
trap 'rm -rf "$SAMPLE_DIR"' EXIT
sips -s format heic src-tauri/icons/icon.png --out "$SAMPLE_DIR/sample.heic" >/dev/null
"$BIN" --smoke-test "$SAMPLE_DIR/sample.heic"

if $install; then
    check_not_running
    echo "==> $INSTALL_TO にインストール"
    rm -rf "$INSTALL_TO"
    ditto "$APP" "$INSTALL_TO"
    # Launch Services に登録して「このアプリケーションで開く」に出るようにする
    "$LSREGISTER" -f "$INSTALL_TO"
fi

echo "完了: $APP ($(du -sh "$APP" | cut -f1))"
