#!/usr/bin/env bash
# 配る用のディスクイメージ（.dmg）を作る。中身は ImageEditorRT.app と /Applications へのリンク
# （開いて .app を Applications にドラッグすればインストールできる）。ほかのアプリの dmg と同じく、開くと
# 大きなアイコンで左にアプリ、右に Applications を並べた小さなウィンドウで出す（Finder に設定させるので、
# 作っている間に Finder のウィンドウが一瞬開く）。
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

VOLUME="ImageEditorRT"
MOUNT="/Volumes/$VOLUME"
if [[ -e "$MOUNT" ]]; then
    echo "$MOUNT がマウントされています。取り出してから、もう一度実行してください。" >&2
    exit 1
fi

STAGE="$(mktemp -d)"
WORK="$(mktemp -d)"
cleanup() {
    if [[ -d "$MOUNT" ]]; then hdiutil detach "$MOUNT" -quiet -force 2>/dev/null || true; fi
    rm -rf "$STAGE" "$WORK"
}
trap cleanup EXIT
# ボリュームの一番上のフォルダの権限になるので、ふつうのフォルダと同じにする
chmod 755 "$STAGE"
ditto "$APP" "$STAGE/ImageEditorRT.app"
ln -s /Applications "$STAGE/Applications"

echo "==> $DMG を作る"
mkdir -p "$OUT_DIR"
rm -f "$DMG"
# 1. 書き込みできるイメージを作ってマウントする（Finder の表示の設定 .DS_Store を書き込むため、少し余裕を持たせる）
SIZE_MB=$(( $(du -sm "$STAGE" | cut -f1) + 20 ))
RW="$WORK/rw.dmg"
hdiutil create -quiet -volname "$VOLUME" -srcfolder "$STAGE" -fs HFS+ -format UDRW -size "${SIZE_MB}m" "$RW"
hdiutil attach -quiet -readwrite -noverify -noautoopen "$RW"

# 2. Finder にウィンドウの見た目を設定させる（ほかのアプリの dmg と同じく、大きなアイコンで左にアプリ、
#    右に Applications。初回は「Finder を操作する」許可を求められる）
osascript >/dev/null <<APPLESCRIPT
tell application "Finder"
    tell disk "$VOLUME"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {200, 120, 860, 520}
        set options to the icon view options of container window
        set arrangement of options to not arranged
        set icon size of options to 128
        set text size of options to 14
        set position of item "ImageEditorRT.app" of container window to {170, 170}
        set position of item "Applications" of container window to {490, 170}
        update without registering applications
        delay 1
        close
    end tell
end tell
APPLESCRIPT
# ボリュームのアイコン（Finder のウィンドウのタイトル・デスクトップに出る）をアプリのアイコンにする
# （元のフォルダに置いても hdiutil create は写さないので、マウントしてから置く。
# Finder に表示を設定させる前に置くと消えてしまったので、その後に置く）
cp src-tauri/icons/icon.icns "$MOUNT/.VolumeIcon.icns"
if command -v SetFile >/dev/null; then SetFile -a C "$MOUNT"; fi
# Finder の変更の記録は配る必要がないので消す
rm -rf "$MOUNT/.fseventsd"
sync
hdiutil detach "$MOUNT" -quiet

# 3. 圧縮した読み取り専用のイメージにする
hdiutil convert -quiet "$RW" -format UDZO -imagekey zlib-level=9 -o "$DMG"
hdiutil verify -quiet "$DMG"

echo "完了: $DMG ($(du -h "$DMG" | cut -f1))"
