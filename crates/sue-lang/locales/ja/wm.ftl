## シェル (crates/sue-wm)。下書き: ネイティブスピーカーの確認待ち。

## Sue メニュー

hub = ハブ
studios = スタジオ
tools = ツール
new = 新規
new-file-kind = 新規{ $category }: { $label }
menu-file = ファイル
menu-project = プロジェクト
menu-send-to = 送る
open-file = ファイルを開く
open-file-ellipsis = ファイルを開く...
open-project = プロジェクトを開く
open-project-ellipsis = プロジェクトを開く...
save-project = プロジェクトを保存
save-project-as = 名前を付けてプロジェクトを保存
save-project-as-ellipsis = 名前を付けてプロジェクトを保存...
settings = 設定
quit-sue = Sue を終了
language-system = システム ({ $language })
language-follow-sue = Sue に合わせる ({ $language })
could-not-change-language = 言語を変更できませんでした: { $error }
layout-automatic = 自動 ({ $layout })
layout-desktop = デスクトップ
layout-tablet = タブレット
layout-phone = スマートフォン
layout-phone-shaped = スマートフォンの形

## ハブとピン

sue-hub-every-studio-tool-files = Sue: ハブ、すべてのスタジオとツール、ファイル、プロジェクト、設定
pinned-sue-opens-here-launch-click = ピン留め中: 起動時にここが開きます。クリックでピンを外します
pin-sue-opens-here-launch = ピン留め: 起動時にここを開く
could-not-pin-it = ピン留めできませんでした: { $error }

## コマンドパレット

type-command-mode-entity-action = コマンド、モード、操作を入力
nothing-matches = 一致するものはありません
go-to-the-hub = ハブへ移動
open = { $name }を開く
pin-open-it-at-launch = { $name }をピン留め: 起動時に開く
unpin-open-hub-launch = ピンを外す: 起動時にハブを開く
open-settings = 設定を開く
undo = 取り消し
redo = やり直し

## ステータスバーとメッセージ

there-no-studio-tool-called = { $id } というスタジオやツールはありません
that-is-gone = それはもうありません
is-not-available = { $command } は使用できません
nothing-to-redo = やり直す操作はありません
nothing-to-undo = 取り消す操作はありません
undo-failed = 取り消せませんでした: { $error }
entities = { $count } 個のエンティティ
undo-depth = 取り消し { $depth }
fps = { $fps } FPS
idle = 待機中

## ウィンドウ、ダイアログ、コントロール

close-esc = 閉じる (Esc)
files = ファイル
pick-a-color = 色を選ぶ
none = なし
curve-pen-pressure = 筆圧
curve-output = 出力
color-wheel-show-triangle = HLS の三角形を表示
color-wheel-show-square = HSV の四角形を表示
window-minimize = 最小化
window-maximize = 最大化
window-restore = 元のサイズに戻す
window-close = 閉じる

## ハブの一覧

ago-just-now = たった今
ago-yesterday = 昨日
ago-minutes = { $count } 分前
ago-hours = { $count } 時間前
ago-days = { $count } 日前
discard = 破棄
really-discard = 本当に破棄
recover = 復元
files-projects-you-open-save-show = 開いたり保存したりしたファイルとプロジェクトがここに表示されます。
in-folder = { $folder } の { $lead }
kept-never-saved = { $value } に保管、未保存
kept-newer-than = { $value } に保管、{ $value2 } より新しい
open-recent = 最近使ったものを開く
opened = 開いた
projects-you-save-sue-menu-project = 保存したプロジェクト (Sue メニュー、プロジェクト、プロジェクトを保存) がここに表示されます。
recent = 最近
saved-2 = 保存
saved-in-folder = {" "}({ $folder })
saved-projects = 保存したプロジェクト
unsaved-work = 未保存の作業
untitled = 無題
work-sue-keeps-you-before-saved = 保存前に Sue が保管している作業がここに表示されます。復元するか破棄できます。

check-for-updates = アップデートを確認
label-work = 作業
transfer-copy = コピー
transfer-export = エクスポート
transfer-import = インポート

## files/: the files browser (copied from paint.ftl, which keeps its own until Sue Paint's start page goes)

start-add-label = ラベルを追加
start-all = すべて
start-cancel = キャンセル
start-clear-completed = 完了した転送履歴を消去
start-data-transfers = データ転送
start-date-modified = 更新日時
start-delete-label = ラベルを削除
start-details = 詳細
start-discard = 保存していない作業を破棄
start-favorite = お気に入り
start-kind = 種類
start-last-worked-on = 最終作業日時
start-move-chosen-here = 選択したプロジェクトをここに配置
start-move-to = 移動先
start-name = 名前
start-new-label = 新規ラベル
start-no-projects = プロジェクトはまだありません。「描く」をクリックして開始してください。開いた作品や保存した作品がここに表示されます。
start-no-transfers = 読み込み、書き出し、コピーの履歴はありません。Sue Paint が開いたファイル、保存したファイル、書き出したファイル、コピーしたファイルがここに表示されます。
start-none-match = 一致する項目がありません。
start-open = 開く
start-really-discard = 本当に破棄しますか
start-really-remove = 本当に削除しますか
start-remove-from-list = リストから削除
start-remove-hint = ファイル自体は削除されず、リストからのみ除外されます
start-rename-label = ラベル名を変更
start-search-projects = プロジェクトを検索
start-select-all = すべて選択
start-show-in-folder = フォルダーに表示
start-sort-direction = 順序を反転
start-studio = スタジオ
start-take-off-label = このラベルを解除
start-thumbnail = サムネイル
start-transfer-cancelled = キャンセル
start-transfer-done = 完了: { $when }
start-transfer-failed = 失敗: { $why }
start-transfer-running = 実行中
start-type = 種類
start-type-picture = 作品
start-type-file = ファイル
start-type-project = プロジェクト
start-type-unsaved = 未保存の作業
start-view-large = 大アイコン
start-view-list = リスト
start-view-small = 小アイコン
start-where = 保存場所
## ui/input.rs, ui/slider.rs

field-clear = クリア
field-choices = 候補
range-start = { $name }の開始
range-end = { $name }の終了
## menubar.rs: the menus every studio shares, and notes on menu items

menu-edit = 編集
menu-view = 表示
menu-window = ウィンドウ
menu-help = ヘルプ
menu-new = 新規...
menu-open = 開く...
menu-open-recent = 最近使ったファイル
menu-close = 閉じる
menu-save = 保存
menu-save-as = 名前を付けて保存...
menu-export = 書き出し...
menu-print = 印刷...
menu-settings = 設定...
menu-back-to-hub = ハブに戻る
menu-quit = Sue を終了
menu-undo = 取り消し
menu-redo = やり直し
menu-cut = 切り取り
menu-copy = コピー
menu-paste = 貼り付け
menu-select-all = すべてを選択
menu-guide = { $studio } ガイド
menu-keyboard-shortcuts = ショートカットキー
menu-check-for-updates = アップデートを確認
menu-about = { $studio } について
menu-note-coming-later = 今後対応予定
menu-note-needs-selection = 選択範囲が必要です
## ui/dialog.rs, ui/toast.rs

ok = OK
cancel = キャンセル
dialog-close = 閉じる
toast-dismiss = 閉じる
notices = 通知
