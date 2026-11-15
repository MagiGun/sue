## Sue's settings (the rice), as Settings shows them. Each message is a
## setting: its path with a dash for the dot. Its value is the setting's
## name, .description the line under it, and each other attribute the name
## of one choice. A test keeps the English in step with the settings the
## code declares.

## [general]

general-confirm_on_exit = Confirm on exit
    .description = Ask before closing the window
general-language = Language
    .description = Sue's language, which every studio and tool follows unless set apart (system follows the system's language)
general-autosave_seconds = Autosave Every
    .description = Keep unsaved work this often, so the Hub can bring it back (s; 0 turns autosave off)

## [launchpad]

launchpad-pinned = Open At Launch
    .description = The studio or tool Sue opens straight into (the pin on its box in the Hub; empty opens the Hub)
launchpad-recent_count = Recent Files on the Hub
    .description = How many recent files and projects the Hub lists (0 hides the list)

## [profile]

profile-hardware = Performance
    .description = How smoothly Sue moves and how much power it uses
    .auto = Automatic
    .high-refresh = Smoothest
    .balanced = Balanced
    .legacy-low-spec = Older computer
    .mobile-battery-saver = Save battery

## [display]

display-frame_rate_cap = Frame rate limit
    .description = Highest frame rate while animating (0 follows the performance preference)
display-ui_scale = Interface scale
    .description = Makes everything larger or smaller, on top of the system's scale
display-interface = Interface
    .description = Which layout Sue uses: Automatic follows the device (touch layouts on tablets and phones)
    .auto = Automatic
    .desktop = Desktop
    .tablet = Tablet
    .phone = Phone
display-phone_frame = Show in a phone-wide column
    .description = On a computer, draw the phone layout in a phone-wide column in the middle of the window
display-gpu_preference = Graphics card
    .description = The graphics card that draws the window; Automatic uses the display's (applies after a restart)
    .auto = Automatic
    .integrated = Integrated
    .discrete = Discrete

## [motion]

motion-instant_transitions = Instant transitions
    .description = Snap pages, menus and windows in 0 ms instead of animating them
motion-animation_speed = Animation speed
    .description = Speed of every spring animation
motion-spring_stiffness = Springiness
    .description = How stiff the spring of every animation is
motion-spring_damping = Bounce
    .description = How quickly every animation stops bouncing (more stops sooner)
motion-settle_grace_ms = Keep drawing after a change (ms)
    .description = How long Sue keeps drawing after the last change, so motion ends smoothly
motion-caret_blink_ms = Caret blink (ms)
    .description = Caret blink interval (0 keeps the caret solid)
motion-smooth_caret = Smooth Caret
    .description = Let the text caret glide to its new place instead of jumping
motion-caret_glide_ms = Caret Glide Time
    .description = How long the caret takes to glide to its new place (ms; less is faster)
motion-message_seconds = How long messages stay
    .description = How long messages stay in the status bar (s)

## [layout]

layout-padding = Padding
    .description = Space inside bars, pills and controls (px)
layout-border_width = Border thickness
    .description = Hairline border thickness (px)
layout-control_radius = Button and field corners
    .description = Corner rounding of buttons and fields (px)
layout-card_radius = Card corners
    .description = Corner rounding of cards, list rows and floating palettes (px)
layout-pill_radius = Window and menu corners
    .description = Corner rounding of windows, menus and pill buttons (px)
layout-pill_height = Button height
    .description = Height of pill buttons (px)
layout-bar_height = Top bar height
    .description = Height of the top bar with the Sue button and the menus (px; touch layouts make it taller)
layout-status_bar = Status bar
    .description = Show the thin status bar along the bottom of the window (messages show as a notice without it)
layout-status_bar_height = Status bar height
    .description = Height of the status bar (px)
layout-settings_sidebar_width = Settings list width
    .description = Width of the category list in Settings (px)
layout-shadow_blur = Shadow blur
    .description = Blur of the shadow under menus and windows (px)

## [colors]

colors-void = Page background
    .description = Behind the Hub, the pages and the canvas
colors-panel = Bars and panels
    .description = The top bar, side panels and palettes
colors-pill = Menus and cards
    .description = Menus, boxes and floating surfaces
colors-hover = Hovered controls
    .description = A control under the pointer
colors-active = Pressed and chosen controls
    .description = A control being pressed, or the one chosen
colors-border = Borders
    .description = Hairline borders
colors-border_active = Borders of focused controls
    .description = Borders of hovered boxes and focused controls
colors-text = Text
    .description = Primary text and active icons
colors-muted = Secondary text
    .description = Hints, descriptions and inactive icons
colors-selection = Selected text
    .description = Text selection highlight
colors-selection_text = Text inside a selection
    .description = The color of selected text
colors-grid = Grid lines
    .description = Grid lines in curve editors
colors-shadow = Shadow
    .description = Shadow under floating surfaces

## [typography]

typography-ui_font = Interface font
    .description = The font of menus, buttons and Settings (bundled)
typography-ui_size = Text size
    .description = Size of the interface's text (px)
typography-small_size = Small text size
    .description = Caption and status bar text size (px)
typography-title_size = Title text size
    .description = Title size of cards, dialogs and sections (px)
typography-heading_size = Heading text size
    .description = Heading size (px)
typography-mono_size = Code text size
    .description = Size of text in a fixed-width font (px)

## [input]

input-wheel_line_px = Mouse wheel step (px)
    .description = How far one notch of the mouse wheel scrolls a list

## [io]

io-jxl_effort = JPEG XL compression effort
    .description = JPEG XL effort for exports (higher is smaller and slower)
io-jpeg_lossless_transcode = Keep JPEGs bit for bit as JPEG XL
    .description = When a recompression codec is installed, JPEG files become JPEG XL with every bit kept
io-jpeg_quality = JPEG quality
    .description = JPEG quality for exports to .jpg
io-byte_map_width = Byte map width
    .description = Columns of the picture Sue draws of a file it cannot read
io-max_ingest_mib = Largest file to open (MiB)
    .description = Sue does not open a file larger than this
io-convert_dir = Folder for exported files
    .description = Folder for exported and batch converted files (empty saves next to the source)

## [audio]

audio-output_device = Output device
    .description = Output device (empty for the system default)
audio-sample_rate = Sample rate
    .description = Sample rate (Hz)
audio-buffer_frames = Buffer size
    .description = Buffer size (frames per callback)
audio-realtime_priority = Real-time priority
    .description = Ask the OS to schedule the audio thread in real time
audio-volume = Volume
    .description = Volume of everything Sue plays

## [packages]

packages-require_signatures = Only signed packages
    .description = Only accept packages with a valid Ed25519 signature

## [updates]

updates-check_automatically = Check automatically
    .description = Look for a newer Sue when it starts, at most once a day

## [sue_paint]

sue_paint-width = Width
    .description = Width of new illustrations (px)
sue_paint-height = Height
    .description = Height of new illustrations (px)
sue_paint-dpi = Resolution
    .description = Resolution of new illustrations (dpi)
sue_paint-materials_dir = Materials folder
    .description = Where Sue Paint keeps its materials and brush tips (empty: its own folder)
sue_paint-save_format = Save format
    .description = How Save keeps a picture: a Clip Studio Paint .clip file, or a Sue project
    .clip = Clip Studio Paint (.clip)
    .sue = Sue project (.sue)
sue_paint-touch_mode = On phones and tablets
    .description = Sue Paint on phones and tablets: Simple Mode or Studio Mode
    .simple = Simple
    .studio = Studio
sue_paint-custom_key_1 = Studio Mode key T1
    .description = The command Studio Mode's key T1 runs
sue_paint-custom_key_2 = Studio Mode key T2
    .description = The command Studio Mode's key T2 runs
sue_paint-custom_key_3 = Studio Mode key T3
    .description = The command Studio Mode's key T3 runs
sue_paint-custom_key_4 = Studio Mode key T4
    .description = The command Studio Mode's key T4 runs
sue_paint-custom_key_5 = Studio Mode key T5
    .description = The command Studio Mode's key T5 runs
sue_paint-custom_key_6 = Studio Mode key T6
    .description = The command Studio Mode's key T6 runs
sue_paint-custom_key_7 = Studio Mode key T7
    .description = The command Studio Mode's key T7 runs
sue_paint-theme = Theme
    .description = Sue Paint's colors: Gray, Sue's own theme (follows it), a preset, or Custom (below)
    .gray = Gray
    .sue-theme = Sue theme
    .void = Void
    .graphite = Graphite
    .paper = Paper
    .custom = Custom
sue_paint-pen_pressure = Pen pressure curve
    .description = The curve Pen Pressure Settings draws, as pressure,output points
sue_paint-shift_line = Shift for straight lines
    .description = Shift and drag draws a straight line, shown as it is drawn
sue_paint-two_finger_tap = Two-finger tap
    .description = What a quick tap of two fingers on the canvas does
    .undo = Undo
    .none = Nothing
sue_paint-three_finger_tap = Three-finger tap
    .description = What a quick tap of three fingers on the canvas does
    .redo = Redo
    .none = Nothing
sue_paint-brush_cursor = Brush cursor
    .description = The pointer over the canvas while painting: the brush's outline, a crosshair or a dot
    .brush = Brush shape
    .crosshair = Crosshair
    .dot = Dot
sue_paint-selection_layer_opacity = Opacity of selection layers
    .description = Opacity of a new selection layer (%)
sue_paint-rotate_step = Rotation step
    .description = How far Rotate Left and Rotate Right turn the view (degrees)
    .description = How far Rotate Left and Rotate Right turn the view (degrees; Preferences, Canvas)
sue_paint-layout_style = Layout Style
    .description = How Sue Paint's workspace is designed: which palettes there are and where they sit (Window, Layout Style)
    .classic = Clip Studio
    .sue_paint = Sue Paint (Coming Later)
sue_paint-remember_layout = Remember the palette layout
    .description = Sue Paint opens with its palettes where they were left (Window, Workspace)
sue_paint-alt_tool = Alt while drawing
    .description = What holding Alt while drawing does on the canvas
    .eyedropper = Eyedropper
    .eraser = Eraser
    .none = None
sue_paint-space_tool = Space
    .description = What holding Space does on the canvas
    .hand = Hand
    .none = None
sue_paint-ctrl_shift = Ctrl+Shift click
    .description = What a Ctrl+Shift click does on the canvas
    .pick_layer = Pick layer
    .none = None
sue_paint-language = Language
    .description = Same as Sue, or a language of its own

## [sue_paint_colors]

sue_paint_colors-void = Around the picture
    .description = The area around the canvas
sue_paint_colors-panel = Palettes and menus
    .description = Palettes, bars and menus
sue_paint_colors-pill = Fields, chips and cards
    .description = Fields, chips and cards on the palettes
sue_paint_colors-hover = Hovered controls
    .description = A control under the pointer
sue_paint_colors-active = Pressed and chosen controls
    .description = A control being pressed, or the one chosen
sue_paint_colors-border = Borders
    .description = Hairline borders
sue_paint_colors-border_active = Borders of focused controls
    .description = Borders of chosen and focused controls
sue_paint_colors-text = Text
    .description = Primary text and active icons
sue_paint_colors-muted = Secondary text
    .description = Hints, descriptions and inactive icons
sue_paint_colors-selection = Selected text
    .description = Text selection highlight
sue_paint_colors-selection_text = Text inside a selection
    .description = The color of selected text
sue_paint_colors-grid = Grids and rulers
    .description = Grid lines and rulers on the canvas
sue_paint_colors-shadow = Shadow
    .description = Shadow under floating surfaces

## [sue_writer]

sue_writer-theme = Theme
    .description = Sue Writer's colors: Sue's own theme (follows it) or a preset
    .sue-theme = Sue theme
    .gray = Gray
    .void = Void
    .graphite = Graphite
    .paper = Paper
sue_writer-smart_quotes = Smart quotes
    .description = Turn straight quotes into curly ones while typing
sue_writer-dark_paper = Dark paper
    .description = Show the page in the theme's dark colors instead of white paper
sue_writer-spell_check = Spell check
    .description = Underline misspelled words while typing
sue_writer-language = Language
    .description = Same as Sue, or a language of its own

## [keys]

keys-command_palette = Command Palette
    .description = Open the command palette
keys-command_palette_alt = Command Palette, Second Key
    .description = Open the command palette (alternative)
keys-open_settings = Open Settings
    .description = Open Settings
keys-open_file = Open a File
    .description = Open a file
keys-save = Save
    .description = Save the project
keys-quit = Quit
    .description = Quit Sue
keys-undo = Undo
    .description = Undo the last change
keys-redo = Redo
    .description = Redo
keys-redo_alt = Redo, Second Key
    .description = Redo (alternative)

## [sue_paint_keys]

sue_paint_keys-help_shortcuts = Help shortcuts
    .description = Keyboard Shortcuts
sue_paint_keys-file_preferences = File preferences
    .description = Preferences
sue_paint_keys-file_shortcuts = File shortcuts
    .description = Shortcut Settings
sue_paint_keys-file_modifiers = File modifiers
    .description = Modifier Key Settings
sue_paint_keys-file_save_as = File save as
    .description = Save As
sue_paint_keys-file_print = File print
    .description = Print
sue_paint_keys-layer_new = Layer new
    .description = Raster Layer
sue_paint_keys-file_new = File new
    .description = New
sue_paint_keys-file_close = File close
    .description = Close
sue_paint_keys-edit_transform_free = Edit transform free
    .description = Free Transform
sue_paint_keys-edit_transform_scale = Edit transform scale
    .description = Scale/Rotate
sue_paint_keys-filter_hue = Filter hue
    .description = Hue/Saturation/Luminosity
sue_paint_keys-select_invert = Select invert
    .description = Invert Selected Area
sue_paint_keys-filter_invert = Filter invert
    .description = Reverse Gradient
sue_paint_keys-edit_cut = Edit cut
    .description = Cut
sue_paint_keys-edit_copy = Edit copy
    .description = Copy
sue_paint_keys-edit_paste = Edit paste
    .description = Paste
sue_paint_keys-edit_paste_here = Edit paste here
    .description = Paste to Shown Position
sue_paint_keys-edit_clear = Edit clear
    .description = Delete
sue_paint_keys-edit_fill = Edit fill
    .description = Fill
sue_paint_keys-edit_clear_outside = Edit clear outside
    .description = Delete Outside Selection
sue_paint_keys-layer_toggle_clip = Layer toggle clip
    .description = Clip to Layer Below
sue_paint_keys-layer_folder_insert = Layer folder insert
    .description = Create Folder and Insert Layer
sue_paint_keys-layer_merge_visible_here = Layer merge visible here
    .description = Merge Visible Layers
sue_paint_keys-layer_ungroup = Layer ungroup
    .description = Ungroup Layer Folder
sue_paint_keys-layer_merge_selected = Layer merge selected
    .description = Merge Selected Layers
sue_paint_keys-layer_merge_down = Layer merge down
    .description = Merge with Layer Below
sue_paint_keys-layer_select_above = Layer select above
    .description = Select Layer Above
sue_paint_keys-layer_select_below = Layer select below
    .description = Select Layer Below
sue_paint_keys-layer_order_up = Layer order up
    .description = Move Up
sue_paint_keys-layer_order_down = Layer order down
    .description = Move Down
sue_paint_keys-layer_toggle_layer_color = Layer toggle layer color
    .description = Switch Using Layer Color
sue_paint_keys-select_again = Select again
    .description = Reselect
sue_paint_keys-select_all = Select all
    .description = Select All
sue_paint_keys-select_none = Select none
    .description = Deselect
sue_paint_keys-view_actual = View actual
    .description = 100%
sue_paint_keys-view_fit = View fit
    .description = Fit to Screen
sue_paint_keys-view_rotate_reset = View rotate reset
    .description = Reset Rotation
sue_paint_keys-view_zoom_in = View zoom in
    .description = Zoom In
sue_paint_keys-view_zoom_out = View zoom out
    .description = Zoom Out
sue_paint_keys-view_ruler_bar = View ruler bar
    .description = Ruler Bar
sue_paint_keys-view_snap_ruler = View snap ruler
    .description = Snap to Ruler
sue_paint_keys-view_snap_special = View snap special
    .description = Snap to Special Ruler
sue_paint_keys-view_snap_grid = View snap grid
    .description = Snap to Grid
sue_paint_keys-view_special_ruler_snapping = View special ruler snapping
    .description = Toggle Special Ruler Snapping
sue_paint_keys-view_rotate_left = View rotate left
    .description = Rotate Left
sue_paint_keys-view_rotate_right = View rotate right
    .description = Rotate Right
sue_paint_keys-window_prev_tab = Window prev tab
    .description = Previous Canvas
sue_paint_keys-window_next_tab = Window next tab
    .description = Next Canvas
sue_paint_keys-window_hide_menus = Window hide menus
    .description = Hide Menu Bar
sue_paint_keys-window_hide_all = Window hide all
    .description = Hide All Palettes
sue_paint_keys-tool_zoom = Tool zoom
    .description = Zoom Tool
sue_paint_keys-tool_hand = Tool hand
    .description = Hand Tool
sue_paint_keys-tool_rotate = Tool rotate
    .description = Rotate Tool
sue_paint_keys-tool_operation = Tool operation
    .description = Operation Tool
sue_paint_keys-tool_move_layer = Tool move layer
    .description = Move Layer Tool
sue_paint_keys-tool_selection = Tool selection
    .description = Selection Tool
sue_paint_keys-tool_auto_select = Tool auto select
    .description = Auto Select Tool
sue_paint_keys-tool_eyedropper = Tool eyedropper
    .description = Eyedropper Tool
sue_paint_keys-tool_pen = Tool pen
    .description = Pen and Pencil Tools
sue_paint_keys-tool_brush = Tool brush
    .description = Brush, Airbrush and Decoration Tools
sue_paint_keys-tool_eraser = Tool eraser
    .description = Eraser Tool
sue_paint_keys-tool_blend = Tool blend
    .description = Blend Tool
sue_paint_keys-tool_fill = Tool fill
    .description = Fill and Gradient Tools
sue_paint_keys-tool_figure = Tool figure
    .description = Figure, Frame Border and Ruler Tools
sue_paint_keys-tool_text = Tool text
    .description = Text and Balloon Tools
sue_paint_keys-tool_correct_line = Tool correct line
    .description = Correct Line Tool
sue_paint_keys-tool_previous_sub = Tool previous sub
    .description = Previous Sub Tool
sue_paint_keys-tool_next_sub = Tool next sub
    .description = Next Sub Tool
sue_paint_keys-color_swap = Color swap
    .description = Switch Between Main and Sub Color
sue_paint_keys-color_transparent = Color transparent
    .description = Drawing or Transparent Color
sue_paint_keys-brush_smaller = Brush smaller
    .description = Smaller Brush
sue_paint_keys-brush_bigger = Brush bigger
    .description = Bigger Brush
sue_paint_keys-custom = Custom
    .description = Keys of commands that have no default key

## [sue_writer_keys]

sue_writer_keys-binder_new_folder = Binder new folder
    .description = New Folder
sue_writer_keys-binder_new_doc = Binder new doc
    .description = New Document
sue_writer_keys-compile_open = Compile open
    .description = Compile
sue_writer_keys-compile_pdf = Compile pdf
    .description = PDF
sue_writer_keys-format_bold = Format bold
    .description = Bold
sue_writer_keys-format_italic = Format italic
    .description = Italic
sue_writer_keys-format_underline = Format underline
    .description = Underline
sue_writer_keys-format_strike = Format strike
    .description = Strikethrough
sue_writer_keys-project_split = Project split
    .description = Split at Selection
sue_writer_keys-format_superscript = Format superscript
    .description = Superscript
sue_writer_keys-format_subscript = Format subscript
    .description = Subscript
sue_writer_keys-format_clear = Format clear
    .description = Clear Direct Formatting
sue_writer_keys-align_center = Align center
    .description = Center
sue_writer_keys-align_left = Align left
    .description = Left
sue_writer_keys-align_right = Align right
    .description = Right
sue_writer_keys-align_justify = Align justify
    .description = Justify
sue_writer_keys-style_heading1 = Style heading1
    .description = Heading 1
sue_writer_keys-style_heading2 = Style heading2
    .description = Heading 2
sue_writer_keys-style_heading3 = Style heading3
    .description = Heading 3
sue_writer_keys-style_body = Style body
    .description = Body Text
sue_writer_keys-insert_footnote = Insert footnote
    .description = Footnote
sue_writer_keys-insert_page_break = Insert page break
    .description = Page Break
sue_writer_keys-insert_link = Insert link
    .description = Link
sue_writer_keys-edit_find = Edit find
    .description = Find
sue_writer_keys-edit_replace = Edit replace
    .description = Replace
sue_writer_keys-edit_find_previous = Edit find previous
    .description = Find Previous
sue_writer_keys-edit_find_next = Edit find next
    .description = Find Next
sue_writer_keys-view_composition = View composition
    .description = Composition Mode
sue_writer_keys-view_document = View document
    .description = Text
sue_writer_keys-view_corkboard = View corkboard
    .description = Corkboard
sue_writer_keys-view_outliner = View outliner
    .description = Outliner
sue_writer_keys-project_snapshot = Project snapshot
    .description = Take Snapshot
sue_writer_keys-list_bullet = List bullet
    .description = Bulleted List
sue_writer_keys-table_insert = Table insert
    .description = Insert Table
sue_writer_keys-custom = Custom
    .description = Keys of commands that have no default key
sue_writer_keys-review_comment = Review comment
    .description = New Comment
sue_writer_keys-insert_endnote = Insert endnote
    .description = Endnote
sue_writer_keys-review_track = Review track
    .description = Record Changes
sue_writer_keys-review_next_misspelling = Review next misspelling
    .description = Next Misspelling
sue_writer_keys-format_grow = Format grow
    .description = Grow Font
sue_writer_keys-format_shrink = Format shrink
    .description = Shrink Font
sue_writer_keys-insert_mark_nbsp = Insert mark nbsp
    .description = Non-breaking Space
sue_writer_keys-file_save_as = File save as
    .description = Save As
sue_paint-tool_color_icon = Tool color icon
    .description = The Tool palette shows the main and sub colors under its tools (Tool palette menu, Show Color Icon)
