---
description: GitHub Issue を 1 件実装して PR を作成する
argument-hint: <issue番号>
---
GitHub Issue #$ARGUMENTS を実装してください。手順は CLAUDE.md の「Git / GitHub ワークフロー」に従います。

1. `gh issue view $ARGUMENTS` で目的・作業内容・受け入れ条件を確認する。依存 Issue が未完了なら、作業を始めずに報告して止まる。
2. 関連する旧版（`/Users/nomura/01_project/ImageEditor`）の `docs/requirements.md` の要件 ID と、移す元のコードを読む。仕様が曖昧な点があれば、推測で進めずに質問する。
3. `git switch main && git pull` の後、`feat/$ARGUMENTS-<短い英語名>` ブランチを作る（種別は内容に合わせる）。
4. 実装とテストを書く。画像処理・EXIF は `crates/core`（Tauri に依存しない）に書く。
5. `cargo fmt --all` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace` → `npm run build` がすべて通るまで直す。
6. Conventional Commits でコミットし、`git push -u origin HEAD`。
7. `gh pr create` で PR を作る。本文は `.github/pull_request_template.md` の形式で、`Closes #$ARGUMENTS`、変更内容、手動確認手順を書く。
8. 受け入れ条件ごとに満たしたかを一覧で報告して終了する。**PR はマージしない。**
