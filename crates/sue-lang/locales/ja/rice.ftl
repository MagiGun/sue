## Sue の設定 (設定画面での表示)。下書き: ネイティブスピーカーの確認待ち。

## [general]

general-confirm_on_exit = 終了時に確認
    .description = ウィンドウを閉じる前に確認する
general-language = 言語
    .description = Sue の言語。独自の言語を設定したもの以外、すべてのスタジオとツールがこれに合わせます (「システム」はシステムの言語に合わせます)

general-autosave_seconds = 自動保存の間隔
    .description = 未保存の作業をこの間隔で保管し、ハブから戻せるようにする (秒、0 で自動保存しない)

## [launchpad]

launchpad-recent_count = ハブの最近使ったファイル
    .description = ハブに表示する最近のファイルとプロジェクトの数 (0 で一覧を隠す)
launchpad-pinned = 起動時に開く
    .description = Sue が直接開くスタジオやツール (ハブのボックスにあるピン。空ならハブを開きます)

## [profile]

profile-hardware = パフォーマンス設定
    .description = フレームの間隔と効果の予算
    .auto = 自動
    .high-refresh = 高リフレッシュレート
    .balanced = バランス
    .legacy-low-spec = 古い低スペック機
    .mobile-battery-saver = モバイル省電力

## [display]

display-frame_rate_cap = フレームレート上限
    .description = アニメーション中の最大フレームレート (0 はパフォーマンス設定に従う)
display-ui_scale = UI の拡大率
    .description = システムの拡大率に加えるインターフェースの拡大率
display-interface = インターフェース
    .description = レイアウト: 自動はデバイスに合わせます (タブレットとスマートフォンではタッチ用)。切り替えるには選んでください
    .auto = 自動
    .desktop = デスクトップ
    .tablet = タブレット
    .phone = スマートフォン
display-phone_frame = スマートフォンの形
    .description = コンピューターでは、スマートフォンのレイアウトをウィンドウ中央のスマートフォン幅の列に描きます
display-gpu_preference = 使う GPU
    .description = ウィンドウを描画する GPU。自動はディスプレイの GPU を使います (再起動後に反映)
    .auto = 自動
    .integrated = 内蔵
    .discrete = 専用

## [motion]

motion-instant_transitions = 即時の切り替え
    .description = ページ、メニュー、ウィンドウをアニメーションせず 0 ms で切り替える
motion-animation_speed = アニメーションの速度
    .description = すべてのばねアニメーションの速度
motion-spring_stiffness = ばねの硬さ
    .description = 各アニメーションのばね定数 k
motion-spring_damping = ばねの減衰
    .description = 各アニメーションの減衰係数 c
motion-settle_grace_ms = 落ち着くまでの猶予 (ms)
    .description = 最後の変化のあと、この時間だけ描画を続ける (ms)
motion-caret_blink_ms = キャレットの点滅 (ms)
    .description = キャレットの点滅間隔 (0 で点滅しない)
motion-smooth_caret = なめらかなキャレット
    .description = テキストのキャレットを跳ばさず新しい位置まで滑らせる
motion-caret_glide_ms = キャレットの移動時間
    .description = キャレットが新しい位置まで滑る時間 (ms、短いほど速い)
motion-message_seconds = メッセージの表示時間
    .description = ステータスバーにメッセージを表示する時間 (秒)

## [layout]

layout-padding = 余白
    .description = バー、ピル、コントロールの内側の余白 (px)
layout-border_width = 枠線の太さ
    .description = 細い枠線の太さ (px)
layout-control_radius = コントロールの角丸
    .description = ボタンと入力欄の角の丸み (px)
layout-pill_radius = ピルの角丸
    .description = ウィンドウ、メニュー、ピル型ボタンの角の丸み (px)
layout-pill_height = ピルの高さ
    .description = ピル型ボタンの高さ (px)
layout-bar_height = バーの高さ
    .description = Sue ボタンとメニューがある上部バーの高さ (px。タッチ用レイアウトでは高くなります)
layout-status_bar_height = ステータスバーの高さ
    .description = ステータスバーの高さ (px)
layout-settings_sidebar_width = 設定のサイドバーの幅
    .description = 設定のカテゴリ一覧の幅 (px)
layout-shadow_blur = 影のぼかし
    .description = メニューとウィンドウの下の影のぼかし (px)

## [colors]

colors-void = 背景
    .description = ページの背景
colors-panel = パネル
    .description = バーとパネル
colors-pill = ピル
    .description = メニュー、ボックス、浮かぶ面
colors-hover = ホバー
    .description = ポインターが乗ったコントロール
colors-active = アクティブ
    .description = 押された、選ばれたコントロール
colors-border = 枠線
    .description = 細い枠線
colors-border_active = アクティブな枠線
    .description = ポインターが乗ったボックスとフォーカスのあるコントロールの枠線
colors-text = テキスト
    .description = 主なテキストとアクティブなアイコン
colors-muted = 控えめ
    .description = 補助的なテキスト
colors-selection = 選択
    .description = テキスト選択の強調
colors-selection_text = 選択中のテキスト
    .description = 選択範囲内のテキスト
colors-grid = グリッド
    .description = カーブエディターのグリッド線
colors-shadow = 影
    .description = 浮かぶ面の下の影

## [typography]

typography-ui_font = UI フォント
    .description = インターフェースのフォント (内蔵)
typography-ui_size = UI の文字サイズ
    .description = インターフェースの文字サイズ (px)
typography-small_size = 小さい文字サイズ
    .description = キャプションとステータスバーの文字サイズ (px)
typography-heading_size = 見出しのサイズ
    .description = 見出しのサイズ (px)
typography-mono_size = 等幅のサイズ
    .description = 等幅テキストのサイズ (px)

## [input]

input-wheel_line_px = ホイール 1 行 (px)
    .description = 一覧でマウスホイール 1 目盛りあたりにスクロールするピクセル数

## [io]

io-jxl_effort = JPEG XL の処理量
    .description = 変換時の JPEG XL の処理量 (高いほど小さく、遅くなります)
io-jpeg_lossless_transcode = JPEG のロスレス変換
    .description = 再圧縮コーデックがある場合、JPEG をビット単位で JPEG XL に再圧縮する
io-jpeg_quality = JPEG の画質
    .description = .jpg への変換時の JPEG の画質
io-byte_map_width = バイトマップの幅
    .description = 不明なファイルのバイトマップの列数
io-max_ingest_mib = 最大サイズ (MiB)
    .description = Sue が開く最大のファイルサイズ (MiB)
io-convert_dir = 変換先フォルダー
    .description = 変換したファイルのフォルダー (空なら元のファイルの隣に保存)

## [audio]

audio-output_device = 出力デバイス
    .description = 出力デバイス (空ならシステムの既定)
audio-sample_rate = サンプリングレート
    .description = サンプリングレート (Hz)
audio-buffer_frames = バッファーサイズ
    .description = バッファーサイズ (1 回のコールバックあたりのフレーム数)
audio-realtime_priority = リアルタイム優先度
    .description = オーディオスレッドをリアルタイムで実行するよう OS に求める
audio-volume = 音量
    .description = Sue が再生するすべての音量

## [packages]

packages-require_signatures = 署名を必須にする
    .description = 有効な Ed25519 署名のあるパッケージだけを受け入れる

## [sue_paint]

sue_paint-width = 幅
    .description = 新しいイラストの幅 (px)
sue_paint-height = 高さ
    .description = 新しいイラストの高さ (px)
sue_paint-dpi = 解像度
    .description = 新しいイラストの解像度 (dpi)
sue_paint-theme = テーマ
    .description = Sue Paint の色: グレー、Sue のテーマ (合わせる)、プリセット、カスタム (下)
    .gray = グレー
    .sue-theme = Sue のテーマ
    .void = ボイド
    .graphite = グラファイト
    .paper = ペーパー
    .custom = カスタム
sue_paint-pen_pressure = 筆圧
    .description = すべてのツールの筆圧カーブ。筆圧,出力 の点で表します (ファイル、筆圧の設定で描けます)
sue_paint-alt_tool = Alt のツール
    .description = 描画中に Alt を押したときのキャンバスでの動作 (ファイル、修飾キーの設定)
    .eyedropper = スポイト
    .eraser = 消しゴム
    .none = なし
sue_paint-space_tool = Space のツール
    .description = キャンバスで Space を押したときの動作 (ファイル、修飾キーの設定)
    .hand = 手のひら
    .none = なし
sue_paint-ctrl_shift = Ctrl+Shift
    .description = キャンバスで Ctrl+Shift+クリックしたときの動作 (ファイル、修飾キーの設定)
    .pick_layer = レイヤーを選ぶ
    .none = なし
sue_paint-language = 言語
    .description = 言語: sue は Sue の言語に合わせ、言語タグを指定すると独自の言語になります

## [sue_paint_colors]

sue_paint_colors-void = 背景
    .description = 画像の周り
sue_paint_colors-panel = パネル
    .description = パレットとメニュー
sue_paint_colors-pill = ピル
    .description = 入力欄、チップ、カード
sue_paint_colors-hover = ホバー
    .description = ポインターが乗ったコントロール
sue_paint_colors-active = アクティブ
    .description = 押された、選ばれたコントロール
sue_paint_colors-border = 枠線
    .description = 細い枠線
sue_paint_colors-border_active = アクティブな枠線
    .description = 選ばれた、フォーカスのあるコントロールの枠線
sue_paint_colors-text = テキスト
    .description = 主なテキストとアクティブなアイコン
sue_paint_colors-muted = 控えめ
    .description = 補助的なテキスト
sue_paint_colors-selection = 選択
    .description = テキスト選択の強調
sue_paint_colors-selection_text = 選択中のテキスト
    .description = 選択範囲内のテキスト
sue_paint_colors-grid = グリッド
    .description = グリッドと定規
sue_paint_colors-shadow = 影
    .description = 浮かぶ面の下の影

## [sue_writer]

sue_writer-theme = テーマ
    .description = Sue Writer の色: Sue のテーマ (合わせる) かプリセット
    .sue-theme = Sue のテーマ
    .gray = グレー
    .void = ボイド
    .graphite = グラファイト
    .paper = ペーパー
sue_writer-smart_quotes = スマート引用符
    .description = 入力中にまっすぐな引用符を曲がった引用符に変える
sue_writer-dark_paper = ダークペーパー
    .description = ページを白い紙ではなくテーマの暗い色で表示する
sue_writer-language = 言語
    .description = 言語: sue は Sue の言語に合わせ、言語タグを指定すると独自の言語になります

## [image_viewer] と [convert]

image_viewer-language = 言語
    .description = 言語: sue は Sue の言語に合わせ、言語タグを指定すると独自の言語になります
convert-language = 言語
    .description = 言語: sue は Sue の言語に合わせ、言語タグを指定すると独自の言語になります

## [keys]

keys-command_palette = コマンドパレット
    .description = コマンドパレットを開く
keys-command_palette_alt = コマンドパレット (別キー)
    .description = コマンドパレットを開く (別キー)
keys-open_settings = 設定を開く
    .description = 設定を開く
keys-open_file = ファイルを開く
    .description = ファイルを開く
keys-save = 保存
    .description = プロジェクトを保存
keys-quit = 終了
    .description = Sue を終了
keys-undo = 取り消し
    .description = 最後の変更を取り消す
keys-redo = やり直し
    .description = やり直し
keys-redo_alt = やり直し (別キー)
    .description = やり直し (別キー)
## [layout]

layout-status_bar = ステータスバー
    .description = ウィンドウ下部に細いステータスバーを表示（非表示時は通知としてメッセージを表示）

## [updates]

updates-check_automatically = 自動的に確認
    .description = 起動時に新しい Sue を確認（1日1回まで）

## [sue_paint]

sue_paint-materials_dir = 素材フォルダー
    .description = Sue Paint が素材とブラシ先端を保存する場所（空の場合は既定のフォルダー）
sue_paint-save_format = 保存形式
    .description = 画像の保存形式: Clip Studio Paint の .clip ファイルまたは Sue プロジェクト
    .clip = Clip Studio Paint (.clip)
    .sue = Sue プロジェクト (.sue)
sue_paint-touch_mode = タッチモード
    .description = スマートフォンやタブレットでの Sue Paint: シンプルモードまたはスタジオモード
    .simple = シンプル
    .studio = スタジオ
sue_paint-custom_key_1 = カスタムキー 1
    .description = スタジオモードのキー T1 が実行するコマンド
sue_paint-custom_key_2 = カスタムキー 2
    .description = スタジオモードのキー T2 が実行するコマンド
sue_paint-custom_key_3 = カスタムキー 3
    .description = スタジオモードのキー T3 が実行するコマンド
sue_paint-custom_key_4 = カスタムキー 4
    .description = スタジオモードのキー T4 が実行するコマンド
sue_paint-custom_key_5 = カスタムキー 5
    .description = スタジオモードのキー T5 が実行するコマンド
sue_paint-custom_key_6 = カスタムキー 6
    .description = スタジオモードのキー T6 が実行するコマンド
sue_paint-custom_key_7 = カスタムキー 7
    .description = スタジオモードのキー T7 が実行するコマンド
sue_paint-theme = テーマ
    .description = Sue Paint のカラー: グレー、Sue のテーマ（追従）、プリセット、カスタム
    .gray = グレー
    .sue-theme = Sue テーマ
    .void = ボイド
    .graphite = グラファイト
    .paper = ペーパー
    .custom = カスタム
sue_paint-pen_pressure = 筆圧検知
    .description = 各ツールの筆圧検知カーブ（ファイル、筆圧検知レベルの調節で描画）
sue_paint-shift_line = Shift で直線
    .description = Shift を押しながらドラッグで直線を描画（環境設定、ツール）
sue_paint-two_finger_tap = 2本指タップ
    .description = キャンバス上での素早い2本指タップの動作（環境設定、タッチジェスチャー）
    .undo = 元に戻す
    .none = なし
sue_paint-three_finger_tap = 3本指タップ
    .description = キャンバス上での素早い3本指タップの動作（環境設定、タッチジェスチャー）
    .redo = やり直し
    .none = なし
sue_paint-brush_cursor = ブラシカーソル
    .description = 描画時のキャンバス上のポインター: ブラシ形状、十字、ドット（環境設定、カーソル）
    .brush = ブラシ形状
    .crosshair = 十字
    .dot = ドット
sue_paint-selection_layer_opacity = 選択範囲レイヤーの不透明度
    .description = 新規選択範囲レイヤーの不透明度（%、環境設定、レイヤー・コマ枠）
sue_paint-rotate_step = 回転ステップ
    .description = 左回転・右回転でビューを回転させる角度（度、環境設定、キャンバス）
sue_paint-alt_tool = Alt ツール
    .description = 描画中に Alt を押したときのキャンバスでの動作（ファイル、修飾キー設定）
    .eyedropper = スポイト
    .eraser = 消しゴム
    .none = なし
sue_paint-space_tool = スペースツール
    .description = キャンバス上でのスペースキーの動作（ファイル、修飾キー設定）
    .hand = 手のひら
    .none = なし
sue_paint-ctrl_shift = Ctrl+Shift
    .description = キャンバス上での Ctrl+Shift クリックの動作（ファイル、修飾キー設定）
    .pick_layer = レイヤーを選択
    .none = なし
sue_paint-language = 言語
    .description = 言語: sue は Sue の言語に従い、言語タグで個別に設定

## [sue_paint_colors]

sue_paint_colors-void = ボイド
    .description = 画像の周囲
sue_paint_colors-panel = パネル
    .description = パレットとメニュー
sue_paint_colors-pill = ピル
    .description = フィールド、チップ、カード
sue_paint_colors-hover = ホバー
    .description = ポインターが重なったコントロール
sue_paint_colors-active = アクティブ
    .description = 押下中または選択されたコントロール
sue_paint_colors-border = ボーダー
    .description = 極細の境界線
sue_paint_colors-border_active = アクティブボーダー
    .description = 選択またはフォーカスされたコントロールの境界線
sue_paint_colors-text = テキスト
    .description = メインテキストとアクティブアイコン
sue_paint_colors-muted = ミュート
    .description = サブテキスト
sue_paint_colors-selection = 選択
    .description = テキスト選択のハイライト
sue_paint_colors-selection_text = 選択テキスト
    .description = 選択範囲内のテキスト
sue_paint_colors-grid = グリッド
    .description = グリッドと定規
sue_paint_colors-shadow = シャドウ
    .description = フローティングサーフェス下の影

## [sue_writer]

sue_writer-theme = テーマ
    .description = Sue Writer のカラー: Sue のテーマ（追従）またはプリセット
    .sue-theme = Sue テーマ
    .gray = グレー
    .void = ボイド
    .graphite = グラファイト
    .paper = ペーパー
sue_writer-smart_quotes = スマートクォート
    .description = 入力中にストレートクォートをカーリークォートに変換
sue_writer-dark_paper = ダークペーパー
    .description = 白い紙の代わりにテーマの暗い色でページを表示
sue_writer-spell_check = スペルチェック
    .description = 入力中にスペルミスの単語に下線を表示
sue_writer-language = 言語
    .description = 言語: sue は Sue の言語に従い、言語タグで個別に設定

## [image_viewer]

image_viewer-language = 言語
    .description = 言語: sue は Sue の言語に従い、言語タグで個別に設定

## [sue_paint_keys]

sue_paint_keys-help_shortcuts = ヘルプ ショートカット
    .description = ショートカットキー設定
sue_paint_keys-file_preferences = ファイル 環境設定
    .description = 環境設定
sue_paint_keys-file_save_as = ファイル 別名で保存
    .description = 別名で保存
sue_paint_keys-file_print = ファイル 印刷
    .description = 印刷
sue_paint_keys-layer_new = レイヤー 新規
    .description = ラスターレイヤー
sue_paint_keys-file_new = ファイル 新規
    .description = 新規
sue_paint_keys-file_close = ファイル 閉じる
    .description = 閉じる
sue_paint_keys-edit_transform_free = 編集 自由変形
    .description = 自由変形
sue_paint_keys-edit_transform_scale = 編集 拡大縮小回転
    .description = 拡大・縮小・回転
sue_paint_keys-filter_hue = フィルター 色相
    .description = 色相・彩度・明度
sue_paint_keys-select_invert = 選択 反転
    .description = 選択範囲を反転
sue_paint_keys-filter_invert = フィルター 反転
    .description = 階調反転
sue_paint_keys-edit_cut = 編集 切り取り
    .description = 切り取り
sue_paint_keys-edit_copy = 編集 コピー
    .description = コピー
sue_paint_keys-edit_paste = 編集 貼り付け
    .description = 貼り付け
sue_paint_keys-edit_clear = 編集 消去
    .description = 消去
sue_paint_keys-edit_fill = 編集 塗りつぶし
    .description = 塗りつぶし
sue_paint_keys-layer_toggle_clip = レイヤー クリッピング切替
    .description = 下のレイヤーでクリッピング
sue_paint_keys-layer_folder_insert = レイヤー フォルダー挿入
    .description = フォルダーを作成してレイヤーを挿入
sue_paint_keys-layer_merge_visible_here = レイヤー 表示レイヤー統合
    .description = 表示レイヤーを統合
sue_paint_keys-layer_ungroup = レイヤー フォルダー解除
    .description = フォルダー解除
sue_paint_keys-layer_merge_selected = レイヤー 選択中レイヤー結合
    .description = 選択中のレイヤーを結合
sue_paint_keys-layer_merge_down = レイヤー 下と結合
    .description = 下のレイヤーと結合
sue_paint_keys-layer_select_above = レイヤー 上を選択
    .description = 上のレイヤーを選択
sue_paint_keys-layer_select_below = レイヤー 下を選択
    .description = 下のレイヤーを選択
sue_paint_keys-layer_order_up = レイヤー 順序を上へ
    .description = レイヤーを上へ
sue_paint_keys-layer_order_down = レイヤー 順序を下へ
    .description = レイヤーを下へ
sue_paint_keys-select_again = 選択 再選択
    .description = 再選択
sue_paint_keys-select_all = 選択 すべて選択
    .description = すべて選択
sue_paint_keys-select_none = 選択 選択解除
    .description = 選択を解除
sue_paint_keys-view_actual = 表示 100%
    .description = 100%
sue_paint_keys-view_fit = 表示 全体表示
    .description = 全体表示
sue_paint_keys-view_rotate_reset = 表示 回転リセット
    .description = 回転をリセット
sue_paint_keys-view_zoom_in = 表示 ズームイン
    .description = ズームイン
sue_paint_keys-view_zoom_out = 表示 ズームアウト
    .description = ズームアウト
sue_paint_keys-view_ruler_bar = 表示 ルーラーバー
    .description = ルーラー
sue_paint_keys-view_snap_ruler = 表示 ルーラースナップ
    .description = ルーラーにスナップ
sue_paint_keys-view_snap_special = 表示 特殊定規スナップ
    .description = 特殊定規にスナップ
sue_paint_keys-view_snap_grid = 表示 グリッドスナップ
    .description = グリッドにスナップ
sue_paint_keys-view_rotate_left = 表示 左回転
    .description = 左回転
sue_paint_keys-view_rotate_right = 表示 右回転
    .description = 右回転
sue_paint_keys-window_prev_tab = ウィンドウ 前のタブ
    .description = 前のキャンバス
sue_paint_keys-window_next_tab = ウィンドウ 次のタブ
    .description = 次のキャンバス
sue_paint_keys-window_hide_menus = ウィンドウ メニューバー隠す
    .description = メニューバーを隠す
sue_paint_keys-window_hide_all = ウィンドウ すべて隠す
    .description = すべてのパレットを隠す
sue_paint_keys-tool_zoom = ツール ズーム
    .description = ズームツール
sue_paint_keys-tool_hand = ツール 手のひら
    .description = 手のひらツール
sue_paint_keys-tool_rotate = ツール 回転
    .description = 回転ツール
sue_paint_keys-tool_operation = ツール 操作
    .description = 操作ツール
sue_paint_keys-tool_move_layer = ツール レイヤー移動
    .description = レイヤー移動ツール
sue_paint_keys-tool_selection = ツール 選択範囲
    .description = 選択範囲ツール
sue_paint_keys-tool_auto_select = ツール 自動選択
    .description = 自動選択ツール
sue_paint_keys-tool_eyedropper = ツール スポイト
    .description = スポイトツール
sue_paint_keys-tool_pen = ツール ペン
    .description = ペン・鉛筆ツール
sue_paint_keys-tool_brush = ツール ブラシ
    .description = ブラシ・エアブラシ・デコレーションツール
sue_paint_keys-tool_eraser = ツール 消しゴム
    .description = 消しゴムツール
sue_paint_keys-tool_blend = ツール 色混ぜ
    .description = 色混ぜツール
sue_paint_keys-tool_fill = ツール 塗りつぶし
    .description = 塗りつぶし・グラデーションツール
sue_paint_keys-tool_figure = ツール 図形
    .description = 図形・コマ枠・定規ツール
sue_paint_keys-tool_text = ツール テキスト
    .description = テキスト・フキダシツール
sue_paint_keys-tool_correct_line = ツール 線修正
    .description = 線修正ツール
sue_paint_keys-tool_previous_sub = ツール 前のサブツール
    .description = 前のサブツール
sue_paint_keys-tool_next_sub = ツール 次のサブツール
    .description = 次のサブツール
sue_paint_keys-color_swap = カラー切替
    .description = メインカラーとサブカラーを切り替え
sue_paint_keys-color_transparent = カラー 透明色
    .description = 描画色と透明色を切り替え
sue_paint_keys-brush_smaller = ブラシサイズ小
    .description = ブラシサイズを小さく
sue_paint_keys-brush_bigger = ブラシサイズ大
    .description = ブラシサイズを大きく

## [sue_writer_keys]

sue_writer_keys-binder_new_folder = バインダー 新規フォルダー
    .description = 新規フォルダー
sue_writer_keys-binder_new_doc = バインダー 新規ドキュメント
    .description = 新規ドキュメント
sue_writer_keys-compile_open = コンパイル 開く
    .description = コンパイル
sue_writer_keys-compile_pdf = コンパイル PDF
    .description = PDF
sue_writer_keys-format_bold = 書式 太字
    .description = 太字
sue_writer_keys-format_italic = 書式 斜体
    .description = 斜体
sue_writer_keys-format_underline = 書式 下線
    .description = 下線
sue_writer_keys-format_strike = 書式 取り消し線
    .description = 取り消し線
sue_writer_keys-project_split = プロジェクト 分割
    .description = 選択位置で分割
sue_writer_keys-format_superscript = 書式 上付き
    .description = 上付き
sue_writer_keys-format_subscript = 書式 下付き
    .description = 下付き
sue_writer_keys-format_clear = 書式 クリア
    .description = 直接書式をクリア
sue_writer_keys-align_center = 配置 中央揃え
    .description = 中央揃え
sue_writer_keys-align_left = 配置 左揃え
    .description = 左揃え
sue_writer_keys-align_right = 配置 右揃え
    .description = 右揃え
sue_writer_keys-align_justify = 配置 両端揃え
    .description = 両端揃え
sue_writer_keys-style_heading1 = スタイル 見出し1
    .description = 見出し 1
sue_writer_keys-style_heading2 = スタイル 見出し2
    .description = 見出し 2
sue_writer_keys-style_heading3 = スタイル 見出し3
    .description = 見出し 3
sue_writer_keys-style_body = スタイル 本文
    .description = 本文
sue_writer_keys-insert_footnote = 挿入 脚注
    .description = 脚注
sue_writer_keys-insert_page_break = 挿入 改ページ
    .description = 改ページ
sue_writer_keys-insert_link = 挿入 リンク
    .description = リンク
sue_writer_keys-edit_find = 編集 検索
    .description = 検索
sue_writer_keys-edit_replace = 編集 置換
    .description = 置換
sue_writer_keys-edit_find_previous = 編集 前を検索
    .description = 前を検索
sue_writer_keys-edit_find_next = 編集 次を検索
    .description = 次を検索
sue_writer_keys-view_composition = 表示 集中モード
    .description = 集中モード
sue_writer_keys-view_document = 表示 ドキュメント
    .description = テキスト
sue_writer_keys-view_corkboard = 表示 コルクボード
    .description = コルクボード
sue_writer_keys-view_outliner = 表示 アウトライナー
    .description = アウトライナー
sue_writer_keys-project_snapshot = プロジェクト スナップショット
    .description = スナップショットを作成
sue_writer_keys-list_bullet = リスト 箇条書き
    .description = 箇条書きリスト
sue_writer_keys-review_comment = 校閲 コメント
    .description = 新規コメント
sue_writer_keys-insert_endnote = 挿入 文末脚注
    .description = 文末脚注
sue_writer_keys-review_track = 校閲 変更の記録
    .description = 変更を記録
sue_writer_keys-review_next_misspelling = 校閲 次のスペルミス
    .description = 次のスペルミス
sue_writer_keys-format_grow = 書式 フォント拡大
    .description = フォントを拡大
sue_writer_keys-format_shrink = 書式 フォント縮小
    .description = フォントを縮小
sue_writer_keys-insert_mark_nbsp = 挿入 改行なしスペース
    .description = 改行なしスペース

## [image_viewer_keys]

image_viewer_keys-viewer_next = ビューアー 次へ
    .description = 次の画像
image_viewer_keys-viewer_previous = ビューアー 前へ
    .description = 前の画像
image_viewer_keys-viewer_fit = ビューアー ウィンドウに合わせる
    .description = ウィンドウに合わせる
image_viewer_keys-viewer_actual = ビューアー 原寸大
    .description = 原寸大
image_viewer_keys-viewer_zoom_in = ビューアー ズームイン
    .description = ズームイン
image_viewer_keys-viewer_zoom_out = ビューアー ズームアウト
    .description = ズームアウト
image_viewer_keys-viewer_rotate_right = ビューアー 右回転
    .description = 右回転
image_viewer_keys-viewer_rotate_left = ビューアー 左回転
    .description = 左回転
image_viewer_keys-viewer_crop = ビューアー トリミング
    .description = トリミング
image_viewer_keys-viewer_save_as = ビューアー 別名で保存
    .description = 別名で保存
