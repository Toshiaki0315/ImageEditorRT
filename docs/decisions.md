# 決めたこと

作り直しの中で決めたことと、その理由。旧版と動きが違うところは「旧版との違い」に書く。

## 色の空間（#5）

- **読み込み**: macOS の ImageIO で、ファイルに埋め込まれた色のプロファイル（ICC）に従って **sRGB に変換** してから扱う。Display P3（iPhone の写真など）・Adobe RGB・CMYK・グレーの画像も、すべて sRGB の RGBA（8bit）になる。プロファイルのない画像は sRGB とみなす。
- **加工・プレビュー**: すべて sRGB の値のまま計算する（旧版の計算式も sRGB を前提にしている）。
- **保存**: sRGB の画像として書き出し、色のプロファイルは付けない（旧版と同じ。プロファイルのない画像は sRGB とみなされる）。
- **理由**: 加工の計算式を旧版とそろえやすく、どのアプリで開いても同じ色に見えるため。P3 の鮮やかな色（sRGB の外の色）は sRGB の範囲に収まる（少し鮮やかさが減る）が、写真の加工アプリとしては許容できる。
- **旧版との違い**: 旧版（Pillow）はプロファイルを使わず、P3 の値をそのまま sRGB の値として扱っていた。そのため P3 の写真は少しくすんで表示・保存されていた。作り直しでは元の色に近くなる。
- テスト: `crates/core/tests/decode_formats.rs` の `display_p3_is_converted_to_srgb`

## プレビューの受け渡し（#1）

- プレビューは RGBA の生のバイト列で渡す（`tauri::ipc::Response`）。JPEG にする時間（10〜15ms）がかからず、6.7MB でも受け渡しは 4ms 前後（`docs/prototype.md`）。

## 保存（#7）

- **形式**: 拡張子で決める（PNG・JPEG・GIF・TIFF・BMP）。HEIC を開いたときの初期の名前は `.jpg`。
- **JPEG**: jpeg-encoder で書き出す。色の間引きは旧版（Pillow）と同じ 4:2:0、品質は 1〜100（既定 90）。透過は白い背景に合成する。
- **PNG・TIFF**: 透過がなければ RGB、あれば RGBA で書く（旧版も画像のモードのまま書いていた）。
- **TIFF**: 自前で書く（圧縮なし・1 ストリップ。EXIF の項目を IFD0 に入れる）。旧版（Pillow）も既定は圧縮なし。
- **EXIF**: 試作の `ExifBlock` で整え、MakerNote を元の位置に置く。JPEG は APP1、PNG は eXIf のチャンク、TIFF は IFD0 に書く。
- **旧版との違い**: 旧版は、元の EXIF の形を `ExifBlock` で読めないときに Pillow で整えて書いていた。作り直しでは、そのときは EXIF なしで保存する（ほとんど起きない）。
- **保存の設定の保存先**: WebView の localStorage（旧版は QSettings）。

## 色の調整（#8）

- **旧版と画素まで同じにする**: 露出・明るさ・コントラスト・色温度・彩度・周辺減光・経年劣化は、旧版（Pillow）と同じ計算・丸め方にし、旧版で作った画像と画素まで一致することをテストで確かめる（`tests/adjust_matches_python.rs`）。
  - 彩度（`ImageEnhance.Color`）: Pillow の RGB → L の整数の係数、`Image.blend` の float（32bit）の計算と切り捨て。Apple Silicon の Pillow は掛け算と足し算を 1 回でまとめる（FMA）ので `mul_add` を使う
  - 周辺減光: Pillow の `radial_gradient`（256×256）を Pillow と同じバイリニアの計算（`resize::resize_gray_bilinear`）で引き伸ばし、`ImageChops.multiply`（切り捨て）で掛ける
  - 経年劣化の粒子: Python の `random.Random(seed).randbytes` と同じ乱数（メルセンヌ・ツイスタ、`pyrandom.rs`）
- 露出〜色温度は、旧版は 1 つずつかけていたが、作り直しでは 1 つの表にまとめてから 1 回でかける（整数の表どうしなので結果は同じ）。

## テイスト（#9）

- **旧版と画素まで同じにする**: 23 種すべてを旧版で作った画像と比べ、画素まで一致することを確かめる（`tests/filters_match_python.rs`）。Pillow の処理は `pillow.rs` にまとめた（`ImageEnhance` の明るさ・コントラスト・彩度、`Image.blend`、`ImageChops.screen`、`ImageOps.colorize`、`convert("L", 行列)`）。
- **ガウスぼかしを Pillow と同じにした**（`blur.rs`）: 試作は箱ぼかし 3 回の近似だったが、Pillow の「端数のある半径の箱ぼかし」（BoxBlur.c）と UnsharpMask をそのまま移した。ディテール・ジオラマもこのぼかしを使う。
- **半径は倍精度で計算してから float（32bit）にする**: 旧版は Python（倍精度）で半径を計算して Pillow に渡していたため。float32 で計算すると、まれに 1 だけ違う画素が出る。
- **JSON の名前**: テイストは旧版と同じ名前（`"high_tone"` など）。旧版のプリセットを読み込むとき（#21）にそのまま使える。

## ディテールと原寸の処理の速さ（#10）

- **ディテール**（シャープ・ぼかし・ノイズ除去）とリサイズ（Lanczos）も、旧版と画素まで同じにした（`tests/detail_matches_python.rs`・`tests/transform_matches_python.rs`）。リサイズは Pillow の計算（アルファを掛けた形で補間する）をそのまま移した（`resize::pillow_resize`）。プレビュー用の縮小だけは、速さを優先して fast_image_resize を使う（旧版も `reducing_gap` で近似していた）。
- **速さ**: 同じ 6000×4000・同じ重い設定（フレーム・形なし）で、旧版 約 2.0 秒 → 約 0.78 秒。12MP では約 0.39 秒。ぼかしは転置をやめ、真ん中の画素は端の判定なしで計算し、縦は横長の帯に分けて並列にした（結果は Pillow と同じまま）。
- 試作の報告の「旧版 0.4 秒」は 12MP の値だった（`docs/prototype.md` に訂正を書いた）。
- 内訳を測るときは `cargo run --release -p imageeditorrt-core --example profile`。

## ジオラマ（#11）

- 旧版と画素まで同じにした（`tests/diorama_matches_python.rs`）。ピントの帯のくっきり残す度合いは 1 列分を計算し、帯に沿う向きに伸ばして `Image.composite` と同じ丸めで混ぜる。
- ガイドの線（実線: くっきり残す範囲の端、破線: ぼけきる位置）の位置は Rust で計算し（`pipeline::diorama_guide`）、画面は SVG で重ねる。色は旧版と同じ（黄色に黒い影）。「トリミング実行」の表示と 100% 表示でのガイドは、それぞれ #12・#19 で足す。

## 回転・反転とトリミング（#12）

- 範囲の計算（ドラッグ・移動・ハンドル・比を保つ・数値の欄・回転したときの範囲の変換）は `core/crop.rs` に置き、画面はマウスの操作と描画だけを行う（1 か所でテストする）。
- 数値の欄は旧版の QSpinBox と同じく入力のたびに反映する。幅・高さが 0 など範囲にならない途中の値は、欄に残したまま「トリミングなし」として扱う。
- 範囲の外を暗くするマスク・枠・ハンドルは、旧版と同じ色・太さで SVG に描く。

## フレーム・形（#13）

- 旧版と画素まで同じにした（`tests/frame_shape_match_python.rs`）: 角丸・円のマスク（縁のアンチエイリアスは ImageMath の float32 と F → L の切り捨てに合わせる）、形の外側を透明にする・白で塗る、フレームの余白、実際に切り抜く範囲（フレームの写真部分・円の比）、保存・切り抜き表示・全体表示の流れ、出力の大きさ。
- 比をフレーム・円に合わせる計算（チェキはドラッグの形で向きを決める）は `core/crop.rs` の `AspectChoice`。
- 全体表示では、形の外側を範囲の外と同じように暗くし、輪郭を線で描く（SVG）。フレームは「トリミング実行」の表示でだけ付ける。
- 文字を「フレームの余白」に描くのは #15。

## 文字・透かし（#15）

- 配置の計算は旧版と同じ（大きさは短辺の 1〜30%、写真の端から短辺の 3% の余白、行間は大きさの 25%、行の間隔は「A」の下端＋行間、収まらなければ小さくする、不透明度、`alpha_composite` と同じ重ね方）。
- **旧版との違い**: 文字の形は Pillow（FreeType）ではなく ab_glyph で描くので、縁のなめらかさや字の位置が 1px 程度違うことがある（画素までは一致させない）。大きさは Pillow と同じく em（1 文字分の高さ）で指定する。
- フォントのファイルが読めないときは、ヒラギノ角ゴシック W3 で描く（旧版は Pillow の組み込みフォントだった）。
- JSON の名前は旧版のプリセットと同じ（フォントは `GOTHIC` など、位置は `bottom_right` など）。
- ダイアログは HTML の dialog を、開いたまま調整できる形（モーダルでない）で開く。

## 主なメーカーの MakerNote（#17）

- 旧版が exifread で読んでいた Canon・Nikon・Sony・Apple (iPhone)・Fujifilm・Olympus・Casio・DJI の MakerNote は、exifread 3.5.1 の読み方（`decode_maker_note`・`dump_ifd`）をそのまま移した（`core/exifread_note.rs`）。exifread が読めなかった（項目が 1 つもない）ときは、これまでどおり `makernote.rs`（ペンタックス・リコー・Samsung など）で読む。
- 値の表示も exifread と同じにする: Python の `str()` の形（`[0, 200]`・`7/2`・`(1.5,)`・`b'...'`）、21 個以上の並びの省略（`..., ... ]`）、表で値の名前を引く、Nikon の露出補正・Olympus の撮影モード・Canon の位置ごとのタグと CameraInfo。exifread の癖（Sony の "SONY DSC" ヘッダーを読み飛ばさない、Apple の値の位置の基準など）もそのまま。
- タグの名前・値の表は exifread の表から自動で作る（`tests/fixtures/makernote_makers/tables.py` → `exifread_tables.rs`。手で直さない）。
- exifread が例外で止まる壊れた MakerNote では、旧版は MakerNote・UserComment・XMP を読まずに読み直していた。これも同じにする（UserComment を出さず、MakerNote は `makernote.rs` で読む）。
- 旧版と同じになることは `tests/makernote_makers_match_python.rs`（19 通り。旧版のテスト部品で作った EXIF を旧版で読んだ結果と比べる）で確かめる。

## ヒストグラム（#18）

- 数え方は旧版と同じにした（`tests/histogram_matches_python.rs`）: 実際に切り抜く範囲（なければ全体）の、形・フレーム・文字を付ける前の写真を数え、透明な画素と形の外側は数えない（アルファ×形のマスクを ImageChops.multiply と同じく切り捨てて 0 なら除く）。輝度は Pillow の L 変換。
- プレビューを描き直すたびに、同じ処理の途中の画像から数える（`pipeline::render_preview_with_histogram`）。画面へは、プレビューの画素のバイト列の後ろに 4 KB（R・G・B・輝度 × 256 個の u32）を付けて渡す。
- 高さの基準は、両端（0・255）を除いた全チャンネルの最大（`Histogram::peak`。画面の `histogramPeak` も同じ）。
- 「表示 > ヒストグラム」(⇧⌘H) はチェックの付くメニュー項目。表示・非表示は画面の環境設定（localStorage）に残し、起動時にメニューのチェックをそれに合わせる（`set_menu_checked`）。
- 加工前の表示（旧版 FR-UI-44）での加工前の分布は、加工前の表示を作る #19 で合わせる。

## 加工前との比較と 100% 表示（#19）

- 加工前の設定は core の `pipeline::before_settings`（向きと、フレーム・円の比に合わせた後の切り抜く範囲だけを残す。旧版の `_before_settings` と同じ）。画面は `comparing` を渡すだけで、Rust が置き換える。ヒストグラムも同じ設定から数えるので、加工前の表示中は加工前の分布になる。
- `\` キーは `keydown`・`keyup` を捕捉段階で受けて既定の動作を止める（入力欄に文字が入らない）。自動リピートは無視し、ウィンドウが非アクティブになったら加工後に戻す。「加工前」ボタンは pointerdown〜pointerup の間だけ。
- 100% 表示は `render_actual_size`（保存と同じ `apply_edits` を別のスレッドで）の結果を、CSS の大きさを `画素 ÷ devicePixelRatio` にして描く（Retina でも 1 画素 = 画面の 1 画素）。原寸の処理が終わるまでは前の表示のまま「100% ・ 更新中…」。設定を変えたら 300ms 落ち着いてから処理し直し、古い依頼の結果は番号で捨てる。
- 見る場所の動かし方・中央へのそろえ方・同じ大きさなら場所を保つ・ダブルクリックでの切り替えは旧版と同じ（`src/zoom.ts`）。「100% で表示」「画面に合わせる」はメニューの項目を使える・使えないにする（`set_menu_enabled`）。
- 100% 表示のジオラマのガイドは、保存結果のうちフレームの余白を除いた写真の部分に対する位置（`pipeline::actual_size_diorama_guide`）。
- 透過の市松模様は、100% 表示ではいつも敷く（形の外側は不透明な画像でも透明になるため）。

## 元に戻す／やり直す・リセット（#20）

- 履歴は `src/history.ts`（`History` と、落ち着いてから積む `HistoryRecorder`）。画面の部品に依存しないので、Node の組み込みのテスト（`npm test`、型を外して読む）で確かめる（`tests-ts/history.test.ts`）。依存パッケージは増やしていない。
- 積む状態は、設定・比の選択と「縦向き」・出力の欄の状態（旧版の PanelState と同じ考え）。出力の幅・高さは出力の欄の状態から決まるので、設定の側には持たない（読み込み直後に値が埋まっても、変更とみなさない）。
- 「元に戻す」「やり直す」はメニューの独自の項目（⌘Z・⇧⌘Z）。戻せないときは使えない状態にするので、そのときの ⌘Z は入力欄に届く。文字・数値の入力欄で編集中は、入力欄の文字に効かせる（旧版の Qt の入力欄と同じ）。
- 未保存の変更は、設定が初期状態とも最後に保存した設定とも違うとき（旧版と同じ）。リセットと、別の画像を開くときに確かめる（旧版と同じ。確認は「破棄」「キャンセル」）。
- リセットは Rust の読み込んだ画像も捨てる（`close_image`）。描きかけのプレビューの結果は描かない。

## プリセット（#21）

- ファイルの形式は旧版と同じ JSON（項目名・テイストなどの名前・並び・字下げまで同じ。旧版で書いたファイルと同じバイト列になることを `tests/presets_match_python.rs` で確かめる）。読み込みの規則（壊れた項目は読み飛ばす、項目がなければ既定値、同じ名前は最初のもの、名前の前後の空白を除いて 50 文字まで）も旧版と同じ。
- **旧版との違い**: 型に収まらない数（負の周辺減光など）は、壊れた項目として読み飛ばす（旧版は読んだまま持ち、使うときに失敗していた）。
- 保存先は `~/Library/Application Support/ImageEditorRT/presets.json`。**旧版のプリセットも使える**: まだこのファイルがなければ旧版の `~/Library/Application Support/ImageEditor/presets.json` を読む（旧版のファイルは書き換えない。保存・削除すると新しい保存先に書く）。
- 「プリセット ▾」のメニューは Rust で作ってボタンの下に出す（`show_preset_menu`。選んだ項目は "menu" のイベントで画面に届く）。JavaScript の Menu API で出したポップアップは、開いている間アプリが応答しなくなったため使わない。
- 名前の入力は HTML の dialog（WebView では prompt が使えないため）。上書き・削除の確認はダイアログ（「上書き」「削除」／「キャンセル」）。
- 当てはめは設定を置き換えてから、フレーム・円の比が変われば手で選んだときと同じく範囲を直す。履歴にはまとめて 1 回の操作として積む。

## まとめて処理（#22）

- 動きは旧版と同じ（`core/batch.rs`）: かける加工はプリセットと同じ組み合わせ（「今の加工」かプリセット）、トリミングと回転・反転はかけない、長辺はフレーム・円の比に切り抜いた後の写真の向きで決める、元と同じ名前・形式で保存し同じ名前・元のファイルそのものなら `_edited`・`_edited_2` …、読めない・保存できない画像は理由を残して続ける、中止は処理中の 1 枚を終えてから、終わったら保存した枚数と処理できなかった画像（最大 10 件）を知らせる。
- 処理は保存と同じスレッドの組で 1 枚ずつ行い、進み具合は "batch-progress" のイベントで画面に送る。実行中は保存・開く・リセットなどを止める（保存中と同じ扱い）。
- ダイアログは HTML の dialog。処理する画像は複数選べる一覧（「取り除く」で外す）、ファイル・フォルダ・保存先はネイティブのダイアログで選ぶ。

## 貼り付け（#23）

- 「編集 > ペースト」(⌘V) は独自の項目にし、何をするかは `src/paste.ts`（旧版と同じ規則。`tests-ts/paste.test.ts`）で決める: Finder でコピーしたファイルはドロップと同じく開き、画像のデータは「クリップボードの画像」（PNG 扱い・EXIF なし・透過は残す）として開く。どちらもなければステータスバーで知らせる。
- 文字・数値の入力欄では、対応形式のファイルか「文字がなく画像がある」ときだけ画像を開き、ほかは入力欄に文字を貼り付ける（`insertText` で入れるので、入力欄の ⌘Z で戻せる）。
- クリップボードは Rust で NSPasteboard を読む（`src-tauri/src/clipboard.rs`。objc2-app-kit・objc2-foundation は Tauri がすでに使っている版）。テストでは本物のクリップボードを書き換えない。
- 貼り付けた画像の保存の初期値は `~/ピクチャ/クリップボード_<日時>.png`（ピクチャがなければホーム。同じ名前があれば `_2` …。名前の決め方は `save::pasted_save_path`、日時は画面のこの Mac の時刻）。まとめて処理の一覧には入れない。

## 仕上げ（#24）

- アイコンは旧版と同じ絵（`scripts/make_icon.py` で 1024px の PNG を描き、`npx tauri icon` で各サイズを作る）。作ったものはリポジトリに入れてあるので、ビルドに Python は要らない。
- ビルドとインストールは `scripts/build_app.sh`（旧版の `build_app.sh` と同じ流れ）: ビルド → ad-hoc 署名 → 起動確認（`--smoke-test`。画面を出さずに、作った PNG と `sips` で作った HEIC を読めるか）→ `--install` なら /Applications に入れて Launch Services に登録。起動中なら入れ替えずに止まる（作業中の画像が失われないよう、アプリは終了させない）。
- 想定外のエラー（旧版 NFR-04）: Rust のパニックはパニックのフックでログ（`~/Library/Logs/ImageEditorRT/imageeditorrt.log`。日時は UTC）に書き、"unexpected-error" のイベントで画面に知らせる。画面の想定外のエラー（`error`・`unhandledrejection`）もログに書く。どちらもダイアログで知らせ、アプリは終わらせない。
- 速さの判定（旧版 NFR-01・02）: `scripts/bench.sh`（計測モード）の最後に出す。NFR-01 は 12MP（4000×3000）の JPEG を開いてプレビューを描き終えるまで、NFR-02 は重い設定のプレビュー更新（受け渡し・描画を含む）の最大。M4 の Mac で NFR-01 は 84ms、NFR-02 は 29ms（どちらも OK）。
- `make dev`・`make app`・`make dmg`（`Makefile`）。dmg は Tauri の dmg の作り方（Finder を AppleScript で動かして見た目を整える）を使わず、`.app` と /Applications へのリンクを入れたフォルダを `diskutil image create from`（古い macOS では `hdiutil create`）で固める（画面の操作がいらず、CI などでも止まらない）。署名は ad-hoc なので、ほかの Mac では Gatekeeper に止められる。

## 未保存のまま終了するときの確認（#47）

- 旧版にはない動き（ユーザーの希望で足した）。⌘Q（メニューの「ImageEditorRT を終了」を独自の項目にした）・ウィンドウを閉じる（赤いボタン・⌘W）・Dock の「終了」などで、未保存の変更（初期状態とも最後に保存した設定とも違う）があれば、開く・リセットのときと同じ確認（「終了」「キャンセル」）を出す。変更がなければそのまま終わる。
- ウィンドウを閉じる求めは画面で止め（`onCloseRequested`）、Dock などからの終了の求め（`RunEvent::ExitRequested`）は Rust で止めて "quit-requested" のイベントで画面に知らせる。画面が確かめた後に `quit_app` で終わる。
- 保存中・まとめて処理中は、ファイルが途中で切れないよう終了しない（終わってから終了してもらう）。

## 既定値の一致のテスト（#51）

- 編集設定の既定値は Rust（`EditSettings::default()`）と TypeScript（`defaultSettings()`・`defaultText()`）に二重に書いている。両方が `tests-ts/fixtures/default-settings.json` と同じかを、それぞれのテスト（`crates/core/tests/default_settings.rs`・`tests-ts/defaults.test.ts`）で確かめる。既定値を変えるときは、両方とこのファイルをそろえる。

## 画面のコードの分け方（#52）

- `main.ts` は入り口だけにし、機能ごとに分けた（`editing`・`view`・`files`・`presetsUi`・`menus`・`status`・`protocol`）。共有の状態・要素・部品は `app.ts` にまとめる（`state`・`dom`・`parts`）。`app.ts` は機能のファイルを読み込まない（部品の知らせは `hooks` で受け、`main.ts` で機能につなぐ）。
- 機能のファイルの中だけで使う状態（100% 表示・加工前の表示・終了の確認中など）は、そのファイルの中に置く。
- メニューの項目の ID は `menus.ts` の `MENU` にまとめる（Rust の `menu.rs`・`presets.rs` と同じ文字列）。
