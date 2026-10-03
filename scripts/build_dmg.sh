#!/usr/bin/env bash
# 配る用のディスクイメージ（.dmg）を作る。中身は ImageEditorRT.app と /Applications へのリンク
# （開いて .app を Applications にドラッグすればインストールできる）。
#
# 使い方:
#   scripts/build_dmg.sh    # 先に scripts/build_app.sh で .app を作っておく（make dmg なら自動で作る）
#
# 署名は ad-hoc なので、ほかの Mac で開くと Gatekeeper に止められる（自分の Mac 用）。
set -euo pipefail

cd "$(dirname "$0")/.."

APP="target/release/bundle/macos/ImageEditorRT.app"
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
OUT_DIR="target/release/bundle/dmg"
DMG="$OUT_DIR/ImageEditorRT_${VERSION}_$(uname -m).dmg"

if [[ ! -d "$APP" ]]; then
    echo "$APP がありません。先に scripts/build_app.sh（make app）を実行してください。" >&2
    exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/ImageEditorRT.app"
ln -s /Applications "$STAGE/Applications"

echo "==> $DMG を作る"
mkdir -p "$OUT_DIR"
rm -f "$DMG"
# 新しい macOS では hdiutil create が非推奨なので diskutil image を使う（なければ hdiutil）
if diskutil image create from --help >/dev/null 2>&1; then
    # 進み具合の表示は出さない（失敗したら知らせる）
    diskutil image create from --format UDZO --volumeName "ImageEditorRT" "$STAGE" "$DMG" >/dev/null 2>&1 ||
        { echo "ディスクイメージを作れません: $DMG" >&2; exit 1; }
else
    hdiutil create -volname "ImageEditorRT" -srcfolder "$STAGE" -format UDZO "$DMG" >/dev/null
fi
hdiutil verify "$DMG" >/dev/null

echo "完了: $DMG ($(du -h "$DMG" | cut -f1))"
