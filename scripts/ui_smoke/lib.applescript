-- 画面の通しの確認で使う、アクセシビリティの操作（開発版のプロセスだけを、プロセス番号で指定して操作する）。
-- 使い方: osascript lib.applescript <操作> <プロセス番号> [引数]

on findElement(e, wantRole, wantTitle, depth)
	tell application "System Events"
		try
			if role of e is wantRole then
				if wantTitle is "" then return e
				set t to ""
				try
					set t to title of e as text
				end try
				if t is wantTitle or t starts with (wantTitle & " ") then return e
			end if
		end try
		if depth > 10 then return missing value
		-- 探している間にシートが開くなどして要素が変わっても止まらないよう、読めない要素は飛ばす
		try
			repeat with c in (UI elements of e)
				set r to my findElement(c, wantRole, wantTitle, depth + 1)
				if r is not missing value then return r
			end repeat
		end try
		return missing value
	end tell
end findElement

on collectTexts(e, depth, out)
	tell application "System Events"
		try
			if role of e is "AXStaticText" then set end of out to (value of e as text)
		end try
		if depth < 10 then
			try
				repeat with c in (UI elements of e)
					set out to my collectTexts(c, depth + 1, out)
				end repeat
			end try
		end if
		return out
	end tell
end collectTexts

on run argv
	set requested to item 1 of argv
	set pid to (item 2 of argv) as integer
	tell application "System Events"
		set p to first process whose unix id is pid
		if requested is "windows" then
			return count of windows of p
		else if requested is "texts" then
			set AppleScript's text item delimiters to linefeed
			set out to {}
			repeat with w in windows of p
				set out to my collectTexts(w, 0, out)
			end repeat
			return out as text
		else if requested is "click" then
			-- ボタン・タブ（ラジオボタン）を名前で押す（シートのダイアログの中も探す）
			set wanted to item 3 of argv
			repeat with w in windows of p
				repeat with r in {"AXButton", "AXRadioButton"}
					set b to my findElement(w, contents of r, wanted, 0)
					if b is not missing value then
						click b
						return "ok"
					end if
				end repeat
			end repeat
			error "見つかりません: " & wanted
		else if requested is "slider" then
			-- スライダーの値（増やす回数を渡すと、その回数だけ増やしてから）
			set s to my findElement(window 1 of p, "AXSlider", item 3 of argv, 0)
			if s is missing value then error "スライダーが見つかりません: " & (item 3 of argv)
			repeat ((item 4 of argv) as integer) times
				perform action "AXIncrement" of s
			end repeat
			return value of s
		else if requested is "menu" then
			click menu item (item 4 of argv) of menu 1 of menu bar item (item 3 of argv) of menu bar 1 of p
			return "ok"
		else if requested is "menu-enabled" then
			return enabled of menu item (item 4 of argv) of menu 1 of menu bar item (item 3 of argv) of menu bar 1 of p
		end if
	end tell
end run
