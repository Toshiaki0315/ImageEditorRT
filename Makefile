# よく使うコマンド。
#   make dev   開発用に起動する（画面の変更はすぐ反映）
#   make app   .app を作る（署名・起動確認まで。scripts/build_app.sh）
#   make dmg   .app を作ってからディスクイメージ（.dmg）にする（scripts/build_dmg.sh）

.PHONY: dev app dmg

# 画面の依存パッケージ（package-lock.json が変わったら入れ直す）
node_modules: package.json package-lock.json
	npm ci
	@touch node_modules

dev: node_modules
	npx tauri dev

app: node_modules
	scripts/build_app.sh

dmg: app
	scripts/build_dmg.sh
