## Sue 的设置 (the rice)。草稿：等待母语者审阅。

## [general]

general-confirm_on_exit = 退出时确认
    .description = 关闭窗口前提示确认
general-language = 语言
    .description = Sue 的主语言，各工作室和工具均默认遵循此语言（“系统”则跟随系统语言）
general-autosave_seconds = 自动保存间隔
    .description = 自动暂存未保存工作的频率，以便在中心恢复（秒；0 表示关闭自动保存）

## [launchpad]

launchpad-recent_count = 中心最近文件数
    .description = 中心列出的最近文件和项目数量（0 表示隐藏列表）
launchpad-pinned = 启动时打开
    .description = Sue 启动后直接打开的工作室或工具（中心方框上的图钉；留空打开中心）

## [profile]

profile-hardware = 性能偏好
    .description = 帧率节奏与特效开销预算
    .auto = 自动
    .high-refresh = 高刷新率
    .balanced = 均衡
    .legacy-low-spec = 旧设备低配置
    .mobile-battery-saver = 移动省电

## [display]

display-frame_rate_cap = 帧率上限
    .description = 动画播放时的最高帧率（0 遵循性能偏好）
display-ui_scale = 界面缩放
    .description = 在系统缩放基础上的界面缩放比例
display-interface = 界面布局
    .description = 布局：自动跟随设备（平板和手机使用触摸布局）；可手动选择切换
    .auto = 自动
    .desktop = 桌面
    .tablet = 平板
    .phone = 手机
display-phone_frame = 手机边框样式
    .description = 在电脑上以窗口居中的手机等宽列绘制手机布局
display-gpu_preference = GPU 偏好
    .description = 负责绘制窗口的显卡；自动使用显示器对应的显卡（重启后生效）
    .auto = 自动
    .integrated = 核芯显卡
    .discrete = 独立显卡

## [motion]

motion-instant_transitions = 即时过渡
    .description = 页面、菜单和窗口在 0 毫秒内瞬间完成切换而不播放动画
motion-animation_speed = 动画速度
    .description = 所有弹簧动画的播放速度
motion-spring_stiffness = 弹簧刚度
    .description = 所有动画的弹簧弹性系数 k
motion-spring_damping = 弹簧阻尼
    .description = 所有动画的阻尼系数 c
motion-settle_grace_ms = 静止缓冲时间 (ms)
    .description = 最后一次改动后保持重绘的时长（毫秒）
motion-caret_blink_ms = 光标闪烁间隔 (ms)
    .description = 文本光标闪烁周期（0 保持光标常亮）
motion-smooth_caret = 平滑光标
    .description = 文本光标平滑滑动至新位置而不是跳变
motion-caret_glide_ms = 光标平滑时间
    .description = 光标平滑滑动至新位置所需时长（毫秒；越小越快）
motion-message_seconds = 状态栏消息时长
    .description = 消息在状态栏保留显示的秒数 (s)

## [layout]

layout-padding = 内边距
    .description = 栏、药丸按钮和控件内部的间距 (px)
layout-border_width = 边框粗细
    .description = 细边框厚度 (px)
layout-control_radius = 控件圆角
    .description = 按钮和输入框的圆角半径 (px)
layout-pill_radius = 药丸圆角
    .description = 窗口、菜单和药丸按钮的圆角半径 (px)
layout-pill_height = 药丸高度
    .description = 药丸按钮的高度 (px)
layout-status_bar = 状态栏
    .description = 在窗口底部显示窄状态栏（隐藏时消息将以通知形式弹出）

## [colors]
colors-void = 纯黑底色
    .description = 页面背景
colors-panel = 面板
    .description = 侧边栏和面板
colors-pill = 药丸控件
    .description = 菜单、对话框和浮动表面
colors-hover = 悬停
    .description = 鼠标悬停控件
colors-active = 激活
    .description = 按下及选中的控件
colors-border = 边框
    .description = 细边框线
colors-border_active = 激活边框
    .description = 悬停框及聚焦控件的边框
colors-text = 文本
    .description = 主要文本和激活图标
colors-muted = 次要文本
    .description = 次要文本
colors-selection = 选中高亮
    .description = 文本选中高亮底色
colors-selection_text = 选中文本
    .description = 选中区域内部文本
colors-shadow = 阴影
    .description = 浮动表面下方的阴影

## [typography]

typography-ui_font = 界面字体
    .description = 界面显示的内置字体
typography-ui_size = 界面字号
    .description = 界面文本字号 (px)
typography-small_size = 小号字体
    .description = 说明文字和状态栏文本字号 (px)
typography-heading_size = 标题字号
    .description = 标题文本字号 (px)
typography-mono_size = 等宽字号
    .description = 等宽代码文本字号 (px)

## [input]

## [io]

## [audio]

audio-buffer_frames = 音频缓冲区帧数
    .description = 每次送往声卡的采样帧数（0 使用声卡默认值）
audio-sample_rate = 音频采样率
    .description = 声音引擎内部采样率（Hz；0 遵循声卡默认值）
## [packages]
## [updates]
updates-check_automatically = 自动检查更新
    .description = 启动 Sue 时检查是否有新版本（最多每天一次）

## [sue_paint]

sue_paint-width = 宽度
    .description = 新建插画的宽度 (px)
sue_paint-height = 高度
    .description = 新建插画的高度 (px)
sue_paint-dpi = 分辨率
    .description = 新建插画的分辨率 (dpi)
sue_paint-materials_dir = 素材文件夹
    .description = Sue Paint 存放素材和笔刷笔尖的路径（留空为自带文件夹）
sue_paint-save_format = 保存格式
    .description = 保存图片的格式：Clip Studio Paint .clip 文件或 Sue 项目
    .clip = Clip Studio Paint (.clip)
    .sue = Sue 项目 (.sue)
sue_paint-touch_mode = 触摸模式
    .description = 手机和平板上的 Sue Paint 模式：简单模式或工作室模式
    .simple = 简单
    .studio = 工作室
sue_paint-custom_key_1 = 自定义键 1
    .description = 工作室模式 T1 键执行的命令
sue_paint-custom_key_2 = 自定义键 2
    .description = 工作室模式 T2 键执行的命令
sue_paint-custom_key_3 = 自定义键 3
    .description = 工作室模式 T3 键执行的命令
sue_paint-custom_key_4 = 自定义键 4
    .description = 工作室模式 T4 键执行的命令
sue_paint-custom_key_5 = 自定义键 5
    .description = 工作室模式 T5 键执行的命令
sue_paint-custom_key_6 = 自定义键 6
    .description = 工作室模式 T6 键执行的命令
sue_paint-custom_key_7 = 自定义键 7
    .description = 工作室模式 T7 键执行的命令
sue_paint-theme = 主题
    .description = Sue Paint 配色：灰色、Sue 自带主题（跟随）、预设或自定义
    .gray = 灰色
    .sue-theme = Sue 主题
    .void = 纯黑
    .graphite = 石墨
    .paper = 纸白
    .custom = 自定义
sue_paint-pen_pressure = 压感调节
    .description = 全局笔刷压感曲线（在“文件” -> “调节笔压检知等级”中绘制）
sue_paint-shift_line = Shift 直线
    .description = 按住 Shift 拖动可绘制直线（首选项 -> 工具）
sue_paint-two_finger_tap = 双指轻点
    .description = 在画布上双指快速轻点的操作（首选项 -> 触摸手势）
    .undo = 撤销
    .none = 无
sue_paint-three_finger_tap = 三指轻点
    .description = 在画布上三指快速轻点的操作（首选项 -> 触摸手势）
    .redo = 重做
    .none = 无
sue_paint-brush_cursor = 笔刷光标
    .description = 绘画时画布上的光标样式：笔刷轮廓、十字准星或圆点（首选项 -> 光标）
    .brush = 笔刷形状
    .crosshair = 十字准星
    .dot = 圆点
sue_paint-selection_layer_opacity = 选择范围图层不透明度
    .description = 新建选择范围图层的不透明度（%，首选项 -> 图层/分镜框）
sue_paint-rotate_step = 旋转步长
    .description = 向左旋转和向右旋转单次转动的角度（度，首选项 -> 画布）
sue_paint-alt_tool = Alt 工具
    .description = 绘画时按住 Alt 在画布上的动作（文件 -> 修饰键设置）
    .eyedropper = 吸管
    .eraser = 橡皮擦
    .none = 无
sue_paint-space_tool = 空格工具
    .description = 在画布上按住空格键的动作（文件 -> 修饰键设置）
    .hand = 抓手
    .none = 无
sue_paint-ctrl_shift = Ctrl+Shift
    .description = 在画布上 Ctrl+Shift 点击的动作（文件 -> 修饰键设置）
    .pick_layer = 选择图层
    .none = 无
sue_paint-language = 语言
    .description = 语言：sue 跟随 Sue 语言，也可通过语言标签单独指定

## [sue_paint_colors]

sue_paint_colors-void = 纯黑底色
    .description = 画布周围空白区域
sue_paint_colors-panel = 面板
    .description = 面板与菜单
sue_paint_colors-pill = 药丸控件
    .description = 输入框、芯片与卡片
sue_paint_colors-hover = 悬停
    .description = 鼠标悬停控件
sue_paint_colors-active = 激活
    .description = 按下及选中的控件
sue_paint_colors-border = 边框
    .description = 细分割线
sue_paint_colors-border_active = 激活边框
    .description = 选中及聚焦控件的边框
sue_paint_colors-text = 文本
    .description = 主文本与激活图标
sue_paint_colors-muted = 次要文本
    .description = 次要说明文本
sue_paint_colors-selection = 选中高亮
    .description = 文本选中高亮底色
sue_paint_colors-selection_text = 选中文本
    .description = 选中区域内部文字
sue_paint_colors-grid = 网格
    .description = 网格与标尺
sue_paint_colors-shadow = 阴影
    .description = 浮动面板下方阴影

## [sue_writer]

sue_writer-theme = 主题
    .description = Sue Writer 配色：Sue 自带主题（跟随）或预设
    .sue-theme = Sue 主题
    .gray = 灰色
    .void = 纯黑
    .graphite = 石墨
    .paper = 纸白
sue_writer-smart_quotes = 智能引号
    .description = 键入时自动将直引号转换为弯引号
sue_writer-dark_paper = 深色纸张
    .description = 页面显示为主题深色背景而非白纸
sue_writer-spell_check = 拼写检查
    .description = 键入时标出拼写错误的单词
sue_writer-language = 语言
    .description = 语言：sue 跟随 Sue 语言，也可单独指定

## [image_viewer]

image_viewer-language = 语言
    .description = 语言：sue 跟随 Sue 语言，也可单独指定

## [convert]

convert-language = 语言
    .description = 语言：sue 跟随 Sue 语言，也可单独指定

## [keys]

keys-command_palette = 命令面板
    .description = 打开命令面板
keys-command_palette_alt = 命令面板 (备选)
    .description = 打开命令面板（备用快捷键）
keys-open_settings = 打开设置
    .description = 打开设置
keys-open_file = 打开文件
    .description = 打开文件
keys-save = 保存
    .description = 保存项目
keys-quit = 退出
    .description = 退出 Sue
keys-undo = 撤销
    .description = 撤销上一步操作
keys-redo = 重做
    .description = 重做
keys-redo_alt = 重做 (备选)
    .description = 重做（备用快捷键）

## [sue_paint_keys]

sue_paint_keys-help_shortcuts = 帮助 快捷键
    .description = 快捷键设置
sue_paint_keys-file_preferences = 文件 首选项
    .description = 首选项
sue_paint_keys-file_save_as = 文件 另存为
    .description = 另存为
sue_paint_keys-file_print = 文件 打印
    .description = 打印
sue_paint_keys-layer_new = 图层 新建栅格图层
    .description = 栅格图层
sue_paint_keys-file_new = 文件 新建
    .description = 新建
sue_paint_keys-file_close = 文件 关闭
    .description = 关闭
sue_paint_keys-edit_transform_free = 编辑 自由变换
    .description = 自由变换
sue_paint_keys-edit_transform_scale = 编辑 缩放旋转
    .description = 放大/缩小/旋转
sue_paint_keys-filter_hue = 滤镜 色相
    .description = 色相/饱和度/明度
sue_paint_keys-select_invert = 选择 反选
    .description = 反选选择范围
sue_paint_keys-filter_invert = 滤镜 渐变反转
    .description = 反转渐变
sue_paint_keys-edit_cut = 编辑 剪切
    .description = 剪切
sue_paint_keys-edit_copy = 编辑 复制
    .description = 复制
sue_paint_keys-edit_paste = 编辑 粘贴
    .description = 粘贴
sue_paint_keys-edit_clear = 编辑 清除
    .description = 删除
sue_paint_keys-edit_fill = 编辑 填充
    .description = 填充
sue_paint_keys-layer_toggle_clip = 图层 剪贴蒙版
    .description = 用下一图层剪裁
sue_paint_keys-layer_folder_insert = 图层 新建图层组
    .description = 创建文件夹并插入图层
sue_paint_keys-layer_merge_visible_here = 图层 合并可见图层
    .description = 合并可见图层
sue_paint_keys-layer_ungroup = 图层 解散图层组
    .description = 解散图层组
sue_paint_keys-layer_merge_selected = 图层 合并选定图层
    .description = 合并选定图层
sue_paint_keys-layer_merge_down = 图层 向下合并
    .description = 与下一图层合并
sue_paint_keys-layer_select_above = 图层 选择上方图层
    .description = 选择上一图层
sue_paint_keys-layer_select_below = 图层 选择下方图层
    .description = 选择下一图层
sue_paint_keys-layer_order_up = 图层 上移
    .description = 向上移动
sue_paint_keys-layer_order_down = 图层 下移
    .description = 向下移动
sue_paint_keys-select_again = 选择 重新选择
    .description = 重新选择
sue_paint_keys-select_all = 选择 全选
    .description = 全选
sue_paint_keys-select_none = 选择 取消选择
    .description = 取消选择
sue_paint_keys-view_actual = 视图 实际大小
    .description = 100%
sue_paint_keys-view_fit = 视图 适合屏幕
    .description = 适合屏幕大小
sue_paint_keys-view_rotate_reset = 视图 重置旋转
    .description = 重置旋转
sue_paint_keys-view_zoom_in = 视图 放大
    .description = 放大
sue_paint_keys-view_zoom_out = 视图 缩小
    .description = 缩小
sue_paint_keys-view_ruler_bar = 视图 标尺栏
    .description = 标尺
sue_paint_keys-view_snap_ruler = 视图 吸附标尺
    .description = 对齐标尺
sue_paint_keys-view_snap_special = 视图 吸附特殊标尺
    .description = 对齐特殊标尺
sue_paint_keys-view_snap_grid = 视图 吸附网格
    .description = 对齐网格
sue_paint_keys-view_rotate_left = 视图 向左旋转
    .description = 向左旋转
sue_paint_keys-view_rotate_right = 视图 向右旋转
    .description = 向右旋转
sue_paint_keys-window_prev_tab = 窗口 上一画布
    .description = 上一画布
sue_paint_keys-window_next_tab = 窗口 下一画布
    .description = 下一画布
sue_paint_keys-window_hide_menus = 窗口 隐藏菜单栏
    .description = 隐藏菜单栏
sue_paint_keys-window_hide_all = 窗口 隐藏全部调色板
    .description = 隐藏全部调色板
sue_paint_keys-tool_zoom = 工具 缩放
    .description = 缩放工具
sue_paint_keys-tool_hand = 工具 抓手
    .description = 抓手工具
sue_paint_keys-tool_rotate = 工具 旋转
    .description = 旋转工具
sue_paint_keys-tool_operation = 工具 操作
    .description = 操作工具
sue_paint_keys-tool_move_layer = 工具 图层移动
    .description = 移动图层工具
sue_paint_keys-tool_selection = 工具 选择范围
    .description = 选择范围工具
sue_paint_keys-tool_auto_select = 工具 自动选择
    .description = 自动选择工具
sue_paint_keys-tool_eyedropper = 工具 吸管
    .description = 吸管工具
sue_paint_keys-tool_pen = 工具 蘸水笔
    .description = 蘸水笔和铅笔工具
sue_paint_keys-tool_brush = 工具 画笔
    .description = 画笔、喷枪和装饰工具
sue_paint_keys-tool_eraser = 工具 橡皮擦
    .description = 橡皮擦工具
sue_paint_keys-tool_blend = 工具 混色
    .description = 混色工具
sue_paint_keys-tool_fill = 工具 填充
    .description = 填充和渐变工具
sue_paint_keys-tool_figure = 工具 图形
    .description = 图形、分镜框和标尺工具
sue_paint_keys-tool_text = 工具 文本
    .description = 文本和气泡工具
sue_paint_keys-tool_correct_line = 工具 线修正
    .description = 修正线宽工具
sue_paint_keys-tool_previous_sub = 工具 上一子工具
    .description = 上一子工具
sue_paint_keys-tool_next_sub = 工具 下一子工具
    .description = 下一子工具
sue_paint_keys-color_swap = 切换颜色
    .description = 切换主色与次色
sue_paint_keys-color_transparent = 透明色
    .description = 绘图色与透明色切换
sue_paint_keys-brush_smaller = 笔刷缩小
    .description = 缩小笔刷尺寸
sue_paint_keys-brush_bigger = 笔刷放大
    .description = 放大笔刷尺寸

## [sue_writer_keys]

sue_writer_keys-binder_new_folder = 活页夹 新建文件夹
    .description = 新建文件夹
sue_writer_keys-binder_new_doc = 活页夹 新建文档
    .description = 新建文档
sue_writer_keys-compile_open = 编译 打开
    .description = 编译
sue_writer_keys-compile_pdf = 编译 PDF
    .description = PDF
sue_writer_keys-format_bold = 格式 加粗
    .description = 加粗
sue_writer_keys-format_italic = 格式 斜体
    .description = 斜体
sue_writer_keys-format_underline = 格式 下划线
    .description = 下划线
sue_writer_keys-format_strike = 格式 删除线
    .description = 删除线
sue_writer_keys-project_split = 项目 分割
    .description = 在选中位置分割
sue_writer_keys-format_superscript = 格式 上标
    .description = 上标
sue_writer_keys-format_subscript = 格式 下标
    .description = 下标
sue_writer_keys-format_clear = 格式 清除
    .description = 清除直接格式
sue_writer_keys-align_center = 对齐 居中
    .description = 居中对齐
sue_writer_keys-align_left = 对齐 居左
    .description = 左对齐
sue_writer_keys-align_right = 对齐 居右
    .description = 右对齐
sue_writer_keys-align_justify = 对齐 两端对齐
    .description = 两端对齐
sue_writer_keys-style_heading1 = 样式 标题1
    .description = 标题 1
sue_writer_keys-style_heading2 = 样式 标题2
    .description = 标题 2
sue_writer_keys-style_heading3 = 样式 标题3
    .description = 标题 3
sue_writer_keys-style_body = 样式 正文
    .description = 正文
sue_writer_keys-insert_footnote = 插入 脚注
    .description = 脚注
sue_writer_keys-insert_page_break = 插入 分页符
    .description = 分页符
sue_writer_keys-insert_link = 插入 链接
    .description = 链接
sue_writer_keys-edit_find = 编辑 查找
    .description = 查找
sue_writer_keys-edit_replace = 编辑 替换
    .description = 替换
sue_writer_keys-edit_find_previous = 编辑 查找上一个
    .description = 查找上一个
sue_writer_keys-edit_find_next = 编辑 查找下一个
    .description = 查找下一个
sue_writer_keys-view_composition = 视图 专注模式
    .description = 专注模式
sue_writer_keys-view_document = 视图 文档
    .description = 文本
sue_writer_keys-view_corkboard = 视图 软木板
    .description = 软木板
sue_writer_keys-view_outliner = 视图 大纲
    .description = 大纲
sue_writer_keys-project_snapshot = 项目 快照
    .description = 创建快照
sue_writer_keys-list_bullet = 列表 项目符号
    .description = 项目符号列表
sue_writer_keys-review_comment = 审阅 批注
    .description = 新建批注
sue_writer_keys-insert_endnote = 插入 尾注
    .description = 尾注
sue_writer_keys-review_track = 审阅 修订
    .description = 记录修订
sue_writer_keys-review_next_misspelling = 审阅 下一拼写错误
    .description = 下一拼写错误
sue_writer_keys-format_grow = 格式 增大字号
    .description = 增大字号
sue_writer_keys-format_shrink = 格式 减小字号
    .description = 减小字号
sue_writer_keys-insert_mark_nbsp = 插入 不换行空格
    .description = 不换行空格

## [image_viewer_keys]

image_viewer_keys-viewer_next = 查看器 下一张
    .description = 下一张图片
image_viewer_keys-viewer_previous = 查看器 上一张
    .description = 上一张图片
image_viewer_keys-viewer_fit = 查看器 适应窗口
    .description = 适应窗口
image_viewer_keys-viewer_actual = 查看器 实际大小
    .description = 实际大小
image_viewer_keys-viewer_zoom_in = 查看器 放大
    .description = 放大
image_viewer_keys-viewer_zoom_out = 查看器 缩小
    .description = 缩小
image_viewer_keys-viewer_rotate_right = 查看器 向右旋转
    .description = 向右旋转
image_viewer_keys-viewer_rotate_left = 查看器 向左旋转
    .description = 向左旋转
image_viewer_keys-viewer_crop = 查看器 裁剪
    .description = 裁剪
image_viewer_keys-viewer_save_as = 查看器 另存为
    .description = 另存为


layout-bar_height = 顶栏高度
    .description = 带有 Sue 按钮和菜单的顶部栏高度（px；触摸布局会更高）

layout-status_bar_height = 状态栏高度
    .description = 底部状态栏高度 (px)

layout-settings_sidebar_width = 设置侧边栏宽度
    .description = 设置中分类列表的宽度 (px)

layout-shadow_blur = 阴影模糊
    .description = 菜单和窗口下方的阴影模糊半径 (px)

colors-grid = 曲线网格
    .description = 曲线编辑器中的网格线

input-wheel_line_px = 滚轮滚动像素
    .description = 列表中鼠标滚轮每刻度滚动的像素数

io-jxl_effort = JPEG XL 编码算力
    .description = 转换时的 JPEG XL 算力消耗（越高文件越小且耗时越长）

io-jpeg_lossless_transcode = JPEG 无损转码
    .description = 安装重压缩编解码器后按位将 JPEG 无损重压缩为 JPEG XL

io-jpeg_quality = JPEG 质量
    .description = 转换为 .jpg 时的 JPEG 图像质量

io-byte_map_width = 字节图宽度
    .description = 未知文件字节图的列数

io-max_ingest_mib = 最大导入大小 (MiB)
    .description = Sue 能够打开的最大文件大小 (MiB)

io-convert_dir = 转换保存文件夹
    .description = 转换后文件的保存目录（留空则保存在源文件旁边）

audio-output_device = 输出设备
    .description = 音频输出设备（留空使用系统默认）

audio-realtime_priority = 实时优先级
    .description = 请求系统以实时优先级调度音频线程

audio-volume = 音量
    .description = Sue 播放声音的主音量

packages-require_signatures = 强制签名验证
    .description = 仅接受带有有效 Ed25519 签名的扩展包
