#!/usr/bin/env bash
# 画面の通しの確認: 開発版を起動し、macOS のアクセシビリティで主な操作をして結果を確かめる。
#
# 使い方:
#   scripts/ui_smoke.sh
#
# - 確かめること: 画像を開く・スライダー・元に戻す／やり直す・色ごとの調整・部分補正（欄）・自動補正・水平の補正（自動を含む）・おまかせ切り抜き・テイストの一覧・加工のコピー＆ペースト・並べて 1 枚に（ダイアログ）・最近使った項目・複数の大きさで保存（ダイアログ）・100% 表示と戻す・左右に分けて比べる・使い方・リセット（キャンセル・破棄）・終了
# - 開発版（npm run tauri dev）を使う（インストールしたアプリと環境設定を共有しないため。画面の環境設定（localStorage）は
#   開発版が ~/Library/WebKit/imageeditorrt/、アプリが ~/Library/WebKit/<識別子>/ と分かれている）
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
# プリセット・最近使った項目は一時フォルダに置く（使っている人のデータに触れない）
(IMAGEEDITORRT_DATA_DIR="$WORK/data" npm run tauri dev -- -- -- "$IMAGE" >"$LOG" 2>&1 &)
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
# 起動直後に押したタブは、画面の準備（前に開いていたタブに戻す）で戻ることがあるので、開き終えてから押し直す
ax click "$PID" "加工" >/dev/null; sleep 0.5

# モーダルのダイアログは、タブを切り替えた後に開くとアクセシビリティに中身が出ない（WebKit の動き）ので、最初に確かめる
ax menu "$PID" "ファイル" "並べて 1 枚に…" >/dev/null
wait_for 10 sh -c "osascript '$LIB' texts $PID | grep -q 'この並べ方では'" || true
check "並べて 1 枚に（ダイアログ）" "この並べ方では 2 枚使います" "$(ax texts "$PID" | grep 'この並べ方では' || true)"
ax click "$PID" "キャンセル" >/dev/null; sleep 0.5

ax menu "$PID" "ファイル" "まとめて処理…" >/dev/null
wait_for 10 sh -c "osascript '$LIB' texts $PID | grep -q '投稿加工'" || true
check "まとめて処理（投稿加工の欄）" "投稿加工" "$(ax texts "$PID" | grep '投稿加工' | head -1 || true)"
ax click "$PID" "キャンセル" >/dev/null; sleep 0.5
ax menu "$PID" "ファイル" "複数の大きさで保存…" >/dev/null; sleep 0.5
check "複数の大きさで保存（大きさの一覧）" "5" "$(ax values "$PID" AXCheckBox | grep -c 'px）=[01]$' || true)"
ax click "$PID" "キャンセル" >/dev/null; sleep 0.5
# 使い方（モーダルのダイアログは、タブを切り替えた後だと中身が AX に出ないので先に確かめる）
ax menu "$PID" "ヘルプ" "ImageEditorRT の使い方" >/dev/null; sleep 0.5
check "使い方（キーボードショートカットの一覧）" "キーボードショートカット" "$(ax texts "$PID" | grep 'キーボードショートカット' || true)"
ax click "$PID" "閉じる" >/dev/null; sleep 0.5
check "使い方を閉じる" "閉じた" "$(ax texts "$PID" | grep -q 'キーボードショートカット' || echo 閉じた)"

check "スライダーを動かす" "3" "$(ax slider "$PID" "明るさ" 3)"
sleep 1 # 変更が落ち着いてから履歴に積まれる
check "元に戻す（メニューが使える）" "true" "$(ax menu-enabled "$PID" "編集" "元に戻す")"
ax menu "$PID" "編集" "元に戻す" >/dev/null; sleep 0.5
check "元に戻す" "0" "$(ax slider "$PID" "明るさ" 0)"
ax menu "$PID" "編集" "やり直す" >/dev/null; sleep 0.5
check "やり直す" "3" "$(ax slider "$PID" "明るさ" 0)"

check "色ごとの調整（色相のスライダー）" "3" "$(ax slider "$PID" "色相" 3)"
ax click "$PID" "色ごとの調整をリセット" >/dev/null; sleep 0.3
check "色ごとの調整をリセット" "0" "$(ax slider "$PID" "色相" 0)"

check "肌をなめらかに（スライダー）" "3" "$(ax slider "$PID" "肌をなめらかに" 3)"
wait_for 15 sh -c "osascript '$LIB' texts $PID | grep -q '顔が見つからない'" || true
check "肌をなめらかに（顔のない画像の知らせ）" "顔が見つからない" "$(ax texts "$PID" | grep '顔が見つからない' || true)"
check "部分補正（範囲の一覧はまだ空）" "範囲=（まだありません）" "$(ax values "$PID" AXPopUpButton | grep '^範囲=' || true)"
ax click "$PID" "円を足す" >/dev/null; sleep 0.3
check "部分補正（円を足すを押した状態）" "円を足す=1" "$(ax values "$PID" AXCheckBox | grep '^円を足す=' || true)"
ax click "$PID" "円を足す" >/dev/null; sleep 0.3
check "部分補正（もう一度押すと戻る）" "円を足す=0" "$(ax values "$PID" AXCheckBox | grep '^円を足す=' || true)"
ax click "$PID" "赤目を補正" >/dev/null; sleep 0.5
check "赤目を補正（チェックが入る・元に戻せる）" "赤目を補正=1 true" "$(ax values "$PID" AXCheckBox | grep '^赤目を補正=' || true) $(ax menu-enabled "$PID" "編集" "元に戻す")"
ax click "$PID" "赤目を補正" >/dev/null; sleep 0.3

ax click "$PID" "自動補正" >/dev/null
wait_for 15 sh -c "osascript '$LIB' texts $PID | grep -q '自動補正:'" || true
check "自動補正（ステータスバー）" "自動補正: 露出" "$(ax texts "$PID" | grep '自動補正:' || true)"

ax click "$PID" "切り抜き" >/dev/null; sleep 0.5
check "水平の補正のスライダー" "0.3" "$(ax slider "$PID" "水平の補正" 3)"
check "切り抜きのガイド（選択肢がある）" "ガイド=" "$(ax values "$PID" AXPopUpButton | grep '^ガイド=' || true)"
ax click "$PID" "傾きを自動で直す" >/dev/null
wait_for 15 sh -c "osascript '$LIB' texts $PID | grep -q '傾き'" || true
check "傾きを自動で直す（ステータスバー）" "傾き" "$(ax texts "$PID" | grep '傾き' || true)"
ax click "$PID" "おまかせ" >/dev/null
wait_for 20 sh -c "osascript '$LIB' texts $PID | grep -q '範囲を選びました\|範囲はそのまま'" || true
check "おまかせ切り抜き（ステータスバー）" "範囲" "$(ax texts "$PID" | grep '目立つ被写体' || true)"
ax menu "$PID" "編集" "元に戻す" >/dev/null; sleep 0.5
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

ax menu "$PID" "編集" "文字・透かし…" >/dev/null
wait_for 10 sh -c "osascript '$LIB' texts $PID | grep -q '（なし）'" || true
check "文字・透かし（ロゴの欄）" "（なし）" "$(ax texts "$PID" | grep '（なし）' | head -1 || true)"
check "文字・透かし（飾り・位置の既定）" "位置=右下 飾り=なし" "$(ax values "$PID" AXPopUpButton | grep -E '^(飾り|位置)=' | sort -u | tr '\n' ' ' | sed 's/ $//' || true)"
ax click "$PID" "撮影日を入れる" >/dev/null; sleep 0.5
check "撮影日を入れる（画像に撮影日時がないので文字は空のまま、元に戻せる）" "true" "$(ax menu-enabled "$PID" "編集" "元に戻す")"
ax click "$PID" "文字を消す" >/dev/null; sleep 0.3
ax click "$PID" "閉じる" >/dev/null; sleep 0.3

check "最近使った項目（開いた画像が載る）" "smoke.png" "$(ax submenu-items "$PID" "ファイル" "最近使った項目")"

ax menu "$PID" "表示" "100% で表示" >/dev/null
wait_for 15 sh -c "[ \"\$(osascript '$LIB' menu-enabled $PID 表示 画面に合わせる)\" = true ]" || true
check "100% で表示" "true" "$(ax menu-enabled "$PID" "表示" "画面に合わせる")"
ax menu "$PID" "表示" "画面に合わせる" >/dev/null; sleep 0.5
check "画面に合わせる" "true" "$(ax menu-enabled "$PID" "表示" "100% で表示")"

ax menu "$PID" "表示" "左右に分けて比べる" >/dev/null; sleep 1
check "左右に分けて比べる（加工後の表示）" "加工後" "$(ax texts "$PID" | grep '加工後' || true)"
ax menu "$PID" "表示" "左右に分けて比べる" >/dev/null; sleep 0.5
check "左右に分けて比べるのをやめる" "やめた" "$(ax texts "$PID" | grep -q '^加工後$' || echo やめた)"

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
