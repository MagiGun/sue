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
