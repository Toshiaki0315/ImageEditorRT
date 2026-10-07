#!/usr/bin/env bash
# 画面の通しの確認: 開発版を起動し、macOS のアクセシビリティで主な操作をして結果を確かめる。
#
# 使い方:
#   scripts/ui_smoke.sh
#
# - 確かめること: 画像を開く・スライダー・元に戻す／やり直す・自動補正・水平の補正（自動を含む）・テイストの一覧・加工のコピー＆ペースト・100% 表示と戻す・リセット（キャンセル・破棄）・終了
# - 開発版（npm run tauri dev）を使う（インストールしたアプリと環境設定を共有しないため）
# - 操作は開発版のプロセスだけに向けたアクセシビリティの操作で行い、キー入力は送らない。クリップボードは触らない
# - ターミナル（またはこのスクリプトを動かすアプリ）に、システム設定の「プライバシーとセキュリティ >
#   アクセシビリティ」の許可が必要。CI では動かさない
set -euo pipefail

cd "$(dirname "$0")/.."

LIB="scripts/ui_smoke/lib.applescript"
WORK="$(mktemp -d)"
LOG="$WORK/dev.log"
PID=""
failures=0

cleanup() {
    if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then kill "$PID" 2>/dev/null || true; fi
    pkill -f "tauri dev" 2>/dev/null || true
    # 開発版の画面を配っていた Vite も止める（このリポジトリのものだけ）
    pkill -f "$PWD/node_modules/.bin/vite" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

ax() { osascript "$LIB" "$@"; }
pass() { echo "  OK  $1"; }
fail() { echo "  NG  $1" >&2; failures=$((failures + 1)); }
check() { # check <説明> <期待> <実際>
    if [[ "$3" == *"$2"* ]]; then pass "$1"; else fail "$1（期待: $2 ／ 実際: $3）"; fi
}
# 条件がそろうまで待つ（秒）
wait_for() {
    local seconds=$1; shift
    for _ in $(seq 1 "$seconds"); do
        if "$@" >/dev/null 2>&1; then return 0; fi
        sleep 1
    done
    return 1
}

if pgrep -f "target/debug/imageeditorrt" >/dev/null; then
    echo "開発版の ImageEditorRT が起動しています。終了してから、もう一度実行してください。" >&2
    exit 1
fi
if ! osascript -e 'tell application "System Events" to get name of first process' >/dev/null 2>&1; then
    echo "アクセシビリティの許可がありません（システム設定 > プライバシーとセキュリティ > アクセシビリティ）。" >&2
    exit 1
fi

# 確かめる画像（512×512 の PNG）
IMAGE="$WORK/smoke.png"
cp src-tauri/icons/icon.png "$IMAGE"

echo "==> 開発版を起動（初回はビルドに数分かかる）"
(npm run tauri dev -- -- -- "$IMAGE" >"$LOG" 2>&1 &)
if ! wait_for 600 pgrep -f "target/debug/imageeditorrt"; then
    echo "開発版が起動しませんでした（$LOG）" >&2
    cat "$LOG" >&2
    exit 1
fi
PID="$(pgrep -f "target/debug/imageeditorrt" | head -1)"
wait_for 60 ax click "$PID" "加工" || { echo "画面が出ませんでした" >&2; exit 1; }

echo "==> 確かめる"
wait_for 30 sh -c "osascript '$LIB' texts $PID | grep -q '原寸 512×512'" || true
check "画像を開く（ステータスバー）" "smoke.png ｜ 原寸 512×512 px" "$(ax texts "$PID" | grep '原寸' || true)"

check "スライダーを動かす" "3" "$(ax slider "$PID" "明るさ" 3)"
sleep 1 # 変更が落ち着いてから履歴に積まれる
check "元に戻す（メニューが使える）" "true" "$(ax menu-enabled "$PID" "編集" "元に戻す")"
ax menu "$PID" "編集" "元に戻す" >/dev/null; sleep 0.5
check "元に戻す" "0" "$(ax slider "$PID" "明るさ" 0)"
ax menu "$PID" "編集" "やり直す" >/dev/null; sleep 0.5
check "やり直す" "3" "$(ax slider "$PID" "明るさ" 0)"

ax click "$PID" "自動補正" >/dev/null
wait_for 15 sh -c "osascript '$LIB' texts $PID | grep -q '自動補正:'" || true
check "自動補正（ステータスバー）" "自動補正: 露出" "$(ax texts "$PID" | grep '自動補正:' || true)"

ax click "$PID" "切り抜き" >/dev/null; sleep 0.5
check "水平の補正のスライダー" "0.3" "$(ax slider "$PID" "水平の補正" 3)"
ax click "$PID" "傾きを自動で直す" >/dev/null
wait_for 15 sh -c "osascript '$LIB' texts $PID | grep -q '傾き'" || true
check "傾きを自動で直す（ステータスバー）" "傾き" "$(ax texts "$PID" | grep '傾き' || true)"
ax click "$PID" "加工" >/dev/null

ax click "$PID" "投稿加工" >/dev/null; sleep 0.5
check "投稿加工のタブ（強さのスライダー）" "50" "$(ax slider "$PID" "強さ" 0 2>&1)"
ax click "$PID" "文字を見つけて隠す" >/dev/null
wait_for 30 sh -c "osascript '$LIB' texts $PID | grep -q '文字'" || true
check "文字を見つけて隠す（ステータスバー）" "文字" "$(ax texts "$PID" | grep '文字' || true)"
ax click "$PID" "加工" >/dev/null

ax click "$PID" "一覧…" >/dev/null
wait_for 15 ax click "$PID" "セピア" || fail "テイストの一覧: 見本が出ない"
sleep 0.5
# 強さのスライダーはテイストが「なし」のとき使えない（アクセシビリティに出ない）ので、出ればセピアを選べている
check "テイストの一覧から選ぶ（強さが使える）" "100" "$(ax slider "$PID" "強さ" 0 2>&1)"

check "加工をペースト（コピーする前は使えない）" "false" "$(ax menu-enabled "$PID" "編集" "加工をペースト")"
ax menu "$PID" "編集" "加工をコピー" >/dev/null; sleep 0.5
check "加工をコピー → ペーストが使える" "true" "$(ax menu-enabled "$PID" "編集" "加工をペースト")"
ax menu "$PID" "編集" "加工をペースト" >/dev/null; sleep 0.5

ax menu "$PID" "表示" "100% で表示" >/dev/null
wait_for 15 sh -c "[ \"\$(osascript '$LIB' menu-enabled $PID 表示 画面に合わせる)\" = true ]" || true
check "100% で表示" "true" "$(ax menu-enabled "$PID" "表示" "画面に合わせる")"
ax menu "$PID" "表示" "画面に合わせる" >/dev/null; sleep 0.5
check "画面に合わせる" "true" "$(ax menu-enabled "$PID" "表示" "100% で表示")"

ax click "$PID" "リセット" >/dev/null
wait_for 10 sh -c "osascript '$LIB' texts $PID | grep -q '保存していない変更があります'" || true
check "リセット: 未保存の変更の確認" "保存していない変更があります" "$(ax texts "$PID" | grep '保存していない' || true)"
ax click "$PID" "キャンセル" >/dev/null; sleep 0.5
check "リセット: キャンセルなら画像はそのまま" "原寸 512×512" "$(ax texts "$PID" | grep '原寸' || true)"
ax click "$PID" "リセット" >/dev/null
wait_for 10 ax click "$PID" "破棄" || fail "リセット: 「破棄」のボタンが出ない"
sleep 1
check "リセット: 破棄で未読込に戻る" "画像が読み込まれていません" "$(ax texts "$PID" | grep '読み込まれていません' || true)"

ax menu "$PID" "ImageEditorRT" "ImageEditorRT を終了" >/dev/null
if wait_for 10 sh -c "! kill -0 $PID"; then pass "終了（変更がなければ確かめずに終わる）"; else fail "終了"; fi

if grep -iE "panic|error\b" "$LOG" | grep -v "IPC custom protocol" >/dev/null; then
    fail "開発版のログにエラーがある（$(grep -iE 'panic|error\b' "$LOG" | grep -v 'IPC custom protocol' | head -3)）"
fi

if ((failures > 0)); then
    echo "失敗: $failures 件" >&2
    exit 1
fi
echo "すべて OK"
